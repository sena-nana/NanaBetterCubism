use super::{
    read::sanitize_response,
    verification::{snapshot_hash, verification_snapshot, verify_postcondition},
    CommandError, EditorService,
};
use crate::domain::{
    EditorConnectionState, EditorEditOutcome, EditorEditResult, OperationAccepted,
    StoredEditorEditPlan,
};
use crate::protocol::RpcClient;
use crate::service::{
    transaction::{mutation_request, require_execution_true, require_true, ExecutionError},
    ActiveOperation,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use uuid::Uuid;

impl EditorService {
    pub(crate) async fn execute_editor_edit(
        &self,
        app: AppHandle,
        preview_id: String,
        cancel: Arc<AtomicBool>,
    ) -> Result<OperationAccepted, CommandError> {
        let (operation_id, plan, rpc) = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "已有 Editor 编辑事务正在执行。",
                ));
            }
            let plan = inner
                .editor_edit_previews
                .remove(&preview_id)
                .ok_or_else(|| CommandError::new("stale_preview", "预览已失效，请重新预览。"))?;
            if plan.preview_id != preview_id
                || plan.generation != inner.generation
                || inner.model_uid.as_deref() != Some(&plan.model_uid)
            {
                return Err(CommandError::new(
                    "stale_preview",
                    "连接或模型已变化，请重新预览。",
                ));
            }
            let rpc = inner
                .rpc
                .clone()
                .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?;
            let operation_id = Uuid::new_v4().simple().to_string();
            inner.operation = Some(ActiveOperation {
                id: operation_id.clone(),
                cancel: cancel.clone(),
            });
            inner.editor_edit_results.insert(
                operation_id.clone(),
                EditorEditResult {
                    operation_id: operation_id.clone(),
                    operation: plan.method.clone(),
                    outcome: EditorEditOutcome::Running,
                    message: "编辑事务正在执行。".into(),
                    verification: None,
                },
            );
            inner.snapshot.state = EditorConnectionState::Editing;
            inner.snapshot.capabilities.batch_create_parameters = false;
            inner.snapshot.capabilities.find_part_parameters = false;
            inner.snapshot.capabilities.official_edit_api = false;
            inner.snapshot.message = format!("正在执行 {}…", plan.method);
            (operation_id, plan, rpc)
        };
        self.emit_snapshot(&app).await;
        let service = self.clone();
        let accepted = OperationAccepted {
            operation_id: operation_id.clone(),
        };
        tokio::spawn(async move {
            service
                .run_editor_edit(app, operation_id, plan, rpc, cancel)
                .await;
        });
        Ok(accepted)
    }

    pub(crate) async fn editor_edit_result(
        &self,
        operation_id: &str,
    ) -> Result<EditorEditResult, CommandError> {
        self.inner
            .lock()
            .await
            .editor_edit_results
            .get(operation_id)
            .cloned()
            .ok_or_else(|| CommandError::new("missing_operation", "没有该编辑操作。"))
    }

    async fn run_editor_edit(
        &self,
        app: AppHandle,
        operation_id: String,
        plan: StoredEditorEditPlan,
        rpc: RpcClient,
        cancel: Arc<AtomicBool>,
    ) {
        let result = self.run_editor_edit_inner(&rpc, &plan, &cancel).await;
        self.finish_editor_edit(&app, &operation_id, result).await;
    }

    pub(super) async fn run_editor_edit_inner(
        &self,
        rpc: &RpcClient,
        plan: &StoredEditorEditPlan,
        cancel: &AtomicBool,
    ) -> EditorEditResult {
        let operation_id = self
            .inner
            .lock()
            .await
            .operation
            .as_ref()
            .map(|operation| operation.id.clone())
            .unwrap_or_default();
        let result = |outcome, message: String, verification| EditorEditResult {
            operation_id: operation_id.clone(),
            operation: plan.method.clone(),
            outcome,
            message,
            verification,
        };
        let current_model = match rpc.request("GetCurrentModelUID", json!({})).await {
            Ok(value) => value,
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        };
        if current_model.get("ModelUID").and_then(Value::as_str) != Some(&plan.model_uid) {
            return result(
                EditorEditOutcome::Failed,
                "当前模型已变化，请重新预览。".into(),
                None,
            );
        }
        let precondition = match verification_snapshot(rpc, &plan.method, &plan.data).await {
            Ok(value) => value,
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        };
        if snapshot_hash(&precondition) != snapshot_hash(&plan.precondition) {
            return result(
                EditorEditOutcome::Failed,
                "目标模型状态已变化，请重新预览。".into(),
                None,
            );
        }
        let mut events = rpc.subscribe();
        match rpc
            .request("NotifyUndoCancel", json!({"Enabled": true}))
            .await
        {
            Ok(value) if value.get("Accepted").and_then(Value::as_bool) == Some(true) => {}
            Ok(_) => {
                return result(
                    EditorEditOutcome::Failed,
                    "Editor 未接受撤销取消通知，未开始编辑。".into(),
                    None,
                );
            }
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        }
        let begin = mutation_request(
            rpc,
            &mut events,
            cancel,
            "EditBegin",
            json!({"Silent": false}),
        )
        .await
        .and_then(require_execution_true);
        if let Err(error) = begin {
            return match error {
                ExecutionError::UserCancelled(true) => result(
                    EditorEditOutcome::CancelledRolledBack,
                    "Editor 已在事务开始前取消操作。".into(),
                    None,
                ),
                ExecutionError::UserCancelled(false) => result(
                    EditorEditOutcome::Unknown,
                    "Editor 通知取消，但未确认恢复结果。".into(),
                    None,
                ),
                error => result(EditorEditOutcome::Failed, error.to_string(), None),
            };
        }
        let mut mutation = mutation_request(
            rpc,
            &mut events,
            cancel,
            "EditSendLog",
            json!({"Message": "正在执行已确认的模型编辑。"}),
        )
        .await
        .map(|_| ());
        if mutation.is_ok() {
            mutation = mutation_request(rpc, &mut events, cancel, &plan.method, plan.data.clone())
                .await
                .and_then(require_execution_true)
                .map(|_| ());
        }
        if mutation.is_ok() {
            mutation = mutation_request(
                rpc,
                &mut events,
                cancel,
                "EditSendProgress",
                json!({"Value": 1.0}),
            )
            .await
            .map(|_| ());
        }
        if let Err(error) = mutation {
            if let ExecutionError::UserCancelled(restored) = error {
                return result(
                    if restored {
                        EditorEditOutcome::CancelledRolledBack
                    } else {
                        EditorEditOutcome::Unknown
                    },
                    if restored {
                        "Editor 已取消并恢复编辑前状态。".into()
                    } else {
                        "Editor 通知取消，但未确认恢复结果。".into()
                    },
                    None,
                );
            }
            let rollback = rpc
                .request("EditEnd", json!({"Cancel": true}))
                .await
                .and_then(require_true);
            return result(
                if rollback.is_ok() {
                    EditorEditOutcome::FailedRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if rollback.is_ok() {
                    format!("编辑失败，Editor 已确认回滚：{error}")
                } else {
                    format!("编辑失败且无法确认回滚：{error}")
                },
                None,
            );
        }
        if cancel.load(Ordering::SeqCst) {
            let rollback = rpc
                .request("EditEnd", json!({"Cancel": true}))
                .await
                .and_then(require_true);
            return result(
                if rollback.is_ok() {
                    EditorEditOutcome::CancelledRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if rollback.is_ok() {
                    "已取消并恢复编辑前状态。".into()
                } else {
                    "已请求取消，但无法确认恢复结果。".into()
                },
                None,
            );
        }
        if let Err(error) = rpc
            .request("EditEnd", json!({"Cancel": false}))
            .await
            .and_then(require_true)
        {
            return result(
                EditorEditOutcome::Unknown,
                format!("无法确认 Editor 是否提交：{error}"),
                None,
            );
        }
        match verification_snapshot(rpc, &plan.method, &plan.data).await {
            Ok(snapshot) => match verify_postcondition(plan, &snapshot) {
                Some(true) => result(
                    EditorEditOutcome::Committed,
                    "Editor 已提交，回读语义验证通过。".into(),
                    Some(sanitize_response(snapshot)),
                ),
                Some(false) => result(
                    EditorEditOutcome::Unknown,
                    "Editor 已结束事务，但回读结果与预览不一致。".into(),
                    Some(sanitize_response(snapshot)),
                ),
                None => result(
                    EditorEditOutcome::Unknown,
                    "Editor 已结束事务，但该参数组合无法可靠回读验证。".into(),
                    Some(sanitize_response(snapshot)),
                ),
            },
            Err(error) => result(
                EditorEditOutcome::Unknown,
                format!("Editor 已结束事务，但回读验证失败：{error}"),
                None,
            ),
        }
    }

    async fn finish_editor_edit(
        &self,
        app: &AppHandle,
        operation_id: &str,
        result: EditorEditResult,
    ) {
        {
            let mut inner = self.inner.lock().await;
            if inner
                .operation
                .as_ref()
                .map(|operation| operation.id.as_str())
                == Some(operation_id)
            {
                inner.operation = None;
            }
            inner
                .editor_edit_results
                .insert(operation_id.into(), result.clone());
            if inner.editor_edit_results.len() > 32 {
                if let Some(key) = inner.editor_edit_results.keys().next().cloned() {
                    inner.editor_edit_results.remove(&key);
                }
            }
            if inner.rpc.is_some() && inner.model_uid.is_some() {
                inner.snapshot.state = EditorConnectionState::Ready;
                inner.snapshot.capabilities.batch_create_parameters = true;
                inner.snapshot.capabilities.find_part_parameters = true;
                inner.snapshot.capabilities.official_api = true;
                inner.snapshot.capabilities.official_edit_api = true;
                inner.snapshot.message = result.message.clone();
            }
        }
        self.emit_snapshot(app).await;
    }
}
