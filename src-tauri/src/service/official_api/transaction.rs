use super::{
    verification::{
        edit_precondition, precondition_snapshot, shared_precondition_snapshot,
        shared_verification_snapshot, verification_snapshot, verify_postcondition,
    },
    CommandError, EditorService,
};
use crate::domain::{
    EditorConnectionState, EditorEditOutcome, EditorEditResult, EditorEditVerification,
    OperationAccepted, StoredEditorEditPlan,
};
use crate::protocol::RpcClient;
use crate::service::{
    insert_bounded_result,
    transaction::{mutation_request, require_execution_true, require_true, ExecutionError},
    ActiveOperation, OperationOwnerKind,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use uuid::Uuid;

pub(super) fn expected_ordered_move_positions(
    plan: &StoredEditorEditPlan,
) -> Option<BTreeMap<String, usize>> {
    match plan.method.as_str() {
        "MoveParameterGroup" => {
            let mut order = plan
                .items
                .first()?
                .precondition
                .get("rootOrder")?
                .as_array()?
                .iter()
                .map(|value| value.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?;
            for item in &plan.items {
                let id = item.data.get("Id")?.as_str()?;
                let index = item.data.get("InsertIndex")?.as_u64()? as usize;
                let current = order.iter().position(|candidate| candidate == id)?;
                order.remove(current);
                if index > order.len() {
                    return None;
                }
                order.insert(index, id.into());
            }
            Some(
                order
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| (id, index))
                    .collect(),
            )
        }
        "MoveParameter" => {
            let mut group_orders = BTreeMap::<String, Vec<String>>::new();
            for item in &plan.items {
                if item.data.get("InsertIndex").is_none() {
                    continue;
                }
                let group_id = item.data.get("GroupId")?.as_str()?.to_string();
                let order = item
                    .precondition
                    .get("destinationOrder")?
                    .as_array()?
                    .iter()
                    .map(|value| value.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?;
                if group_orders
                    .insert(group_id.clone(), order.clone())
                    .is_some_and(|existing| existing != order)
                {
                    return None;
                }
            }
            for item in &plan.items {
                let id = item.data.get("Id")?.as_str()?;
                for order in group_orders.values_mut() {
                    if let Some(current) = order.iter().position(|candidate| candidate == id) {
                        order.remove(current);
                    }
                }
                if let Some(index) = item.data.get("InsertIndex").and_then(Value::as_u64) {
                    let group_id = item.data.get("GroupId")?.as_str()?;
                    let order = group_orders.get_mut(group_id)?;
                    let index = index as usize;
                    if index > order.len() {
                        return None;
                    }
                    order.insert(index, id.into());
                } else if item
                    .data
                    .get("GroupId")
                    .and_then(Value::as_str)
                    .is_some_and(|group_id| group_orders.contains_key(group_id))
                {
                    return None;
                }
            }
            Some(
                group_orders
                    .into_values()
                    .flat_map(|order| order.into_iter().enumerate().map(|(index, id)| (id, index)))
                    .collect(),
            )
        }
        _ => None,
    }
}

struct EditResultContext<'a> {
    operation_id: &'a str,
    method: &'a str,
    total: usize,
}

impl EditResultContext<'_> {
    fn result(
        &self,
        outcome: EditorEditOutcome,
        message: impl Into<String>,
        completed: usize,
        verification: Option<EditorEditVerification>,
    ) -> EditorEditResult {
        EditorEditResult {
            operation_id: self.operation_id.into(),
            operation: self.method.into(),
            outcome,
            message: message.into(),
            completed,
            total: self.total,
            failure_code: None,
            verification,
        }
    }

    fn conflict(&self, message: String) -> EditorEditResult {
        let mut result = self.result(EditorEditOutcome::Failed, message, 0, None);
        result.failure_code = Some("precondition_conflict".into());
        result
    }

    fn cancelled_before_begin(&self) -> EditorEditResult {
        self.result(
            EditorEditOutcome::Failed,
            "操作在事务开始前已取消。",
            0,
            None,
        )
    }

    async fn transaction_error(
        &self,
        rpc: &RpcClient,
        error: ExecutionError,
        completed: usize,
        context: String,
    ) -> EditorEditResult {
        let app_cancelled = matches!(&error, ExecutionError::AppCancelled);
        if let ExecutionError::UserCancelled(restored) = error {
            return self.result(
                if restored {
                    EditorEditOutcome::CancelledRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if restored {
                    format!("{context}：Editor 已取消并恢复编辑前状态。")
                } else {
                    format!("{context}：Editor 通知取消，但未确认恢复结果。")
                },
                completed,
                None,
            );
        }
        let rollback_confirmed = rpc
            .request("EditEnd", json!({"Cancel": true}))
            .await
            .and_then(require_true)
            .is_ok();
        self.result(
            if rollback_confirmed {
                if app_cancelled {
                    EditorEditOutcome::CancelledRolledBack
                } else {
                    EditorEditOutcome::FailedRolledBack
                }
            } else {
                EditorEditOutcome::Unknown
            },
            if rollback_confirmed {
                format!("{context}，Editor 已确认整批回滚：{error}")
            } else {
                format!("{context}且无法确认整批回滚：{error}")
            },
            completed,
            None,
        )
    }
}

impl EditorService {
    pub(crate) async fn execute_editor_edit(
        &self,
        app: AppHandle,
        preview_id: String,
        cancel: Arc<AtomicBool>,
    ) -> Result<OperationAccepted, CommandError> {
        let operation_id = Uuid::new_v4().simple().to_string();
        let permit = self
            .operation_coordinator
            .try_acquire(OperationOwnerKind::EditorTransaction, &operation_id)
            .map_err(|_| {
                CommandError::new(
                    "operation_active",
                    "已有 Editor 编辑事务或电脑代理操作正在执行。",
                )
            })?;
        let (plan, rpc) = {
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
            inner.operation = Some(ActiveOperation {
                id: operation_id.clone(),
                cancel: cancel.clone(),
                _permit: permit,
            });
            insert_bounded_result(
                &mut inner.editor_edit_results,
                operation_id.clone(),
                EditorEditResult {
                    operation_id: operation_id.clone(),
                    operation: plan.method.clone(),
                    outcome: EditorEditOutcome::Running,
                    message: "编辑事务正在执行。".into(),
                    completed: 0,
                    total: plan.items.len(),
                    failure_code: None,
                    verification: None,
                },
            );
            inner.snapshot.state = EditorConnectionState::Editing;
            inner.snapshot.capabilities.batch_create_parameters = false;
            inner.snapshot.capabilities.find_part_parameters = false;
            inner.snapshot.capabilities.official_edit_api = false;
            inner.snapshot.message =
                format!("正在执行 {}（共 {} 项）…", plan.method, plan.items.len());
            (plan, rpc)
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
        let results = EditResultContext {
            operation_id: &operation_id,
            method: &plan.method,
            total: plan.items.len(),
        };
        let total = results.total;
        if cancel.load(Ordering::SeqCst) {
            return results.cancelled_before_begin();
        }
        let current_model = match rpc.request("GetCurrentModelUID", json!({})).await {
            Ok(value) => value,
            Err(error) => {
                return results.result(EditorEditOutcome::Failed, error.to_string(), 0, None);
            }
        };
        if current_model.get("ModelUID").and_then(Value::as_str) != Some(&plan.model_uid) {
            return results.result(
                EditorEditOutcome::Failed,
                "当前模型已变化，请重新预览。",
                0,
                None,
            );
        }
        let shared_precondition =
            match shared_precondition_snapshot(rpc, &plan.method, &plan.model_uid).await {
                Ok(value) => value,
                Err(error) => {
                    return results.result(
                        EditorEditOutcome::Failed,
                        format!("前置结构读取失败：{error}"),
                        0,
                        None,
                    );
                }
            };
        for (index, item) in plan.items.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return results.cancelled_before_begin();
            }
            let snapshot = match precondition_snapshot(
                rpc,
                &plan.method,
                &item.data,
                shared_precondition.as_ref(),
            )
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    return results.result(
                        EditorEditOutcome::Failed,
                        format!("第 {} 项前置状态读取失败：{error}", index + 1),
                        0,
                        None,
                    );
                }
            };
            let precondition = match edit_precondition(&plan.method, &item.data, &snapshot) {
                Ok(value) => value,
                Err(error) if error.invalid_target => {
                    return results.conflict(format!(
                        "第 {} 项目标状态与预览不再一致：{}",
                        index + 1,
                        error.message
                    ));
                }
                Err(error) => {
                    return results.result(
                        EditorEditOutcome::Failed,
                        format!("第 {} 项前置条件校验失败：{}", index + 1, error.message),
                        0,
                        None,
                    );
                }
            };
            if precondition != item.precondition {
                return results.conflict(format!(
                    "第 {} 项目标状态与预览不再一致，整批未开始。",
                    index + 1
                ));
            }
        }
        let mut events = rpc.subscribe();
        match rpc
            .request("NotifyUndoCancel", json!({"Enabled": true}))
            .await
        {
            Ok(value) if value.get("Accepted").and_then(Value::as_bool) == Some(true) => {}
            Ok(_) => {
                return results.result(
                    EditorEditOutcome::Failed,
                    "Editor 未接受撤销取消通知，未开始编辑。",
                    0,
                    None,
                );
            }
            Err(error) => {
                return results.result(EditorEditOutcome::Failed, error.to_string(), 0, None);
            }
        }
        if cancel.load(Ordering::SeqCst) {
            return results.cancelled_before_begin();
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
                ExecutionError::UserCancelled(true) => results.result(
                    EditorEditOutcome::CancelledRolledBack,
                    "Editor 已在事务开始前取消操作。",
                    0,
                    None,
                ),
                ExecutionError::UserCancelled(false) => results.result(
                    EditorEditOutcome::Unknown,
                    "Editor 通知取消，但未确认恢复结果。",
                    0,
                    None,
                ),
                error @ ExecutionError::AppCancelled => {
                    results
                        .transaction_error(rpc, error, 0, "事务开始时取消".into())
                        .await
                }
                ExecutionError::Rpc(error) if error.is_transport_failure() => {
                    results
                        .transaction_error(
                            rpc,
                            ExecutionError::Rpc(error),
                            0,
                            "事务开始状态不确定".into(),
                        )
                        .await
                }
                error => results.result(EditorEditOutcome::Failed, error.to_string(), 0, None),
            };
        }
        let log = mutation_request(
            rpc,
            &mut events,
            cancel,
            "EditSendLog",
            json!({"Message": "正在执行已确认的模型编辑。"}),
        )
        .await
        .map(|_| ());
        if let Err(error) = log {
            return results
                .transaction_error(rpc, error, 0, "编辑准备失败".into())
                .await;
        }

        let mut completed = 0;
        for (index, item) in plan.items.iter().enumerate() {
            let mutation =
                mutation_request(rpc, &mut events, cancel, &plan.method, item.data.clone())
                    .await
                    .and_then(require_execution_true);
            if let Err(error) = mutation {
                return results
                    .transaction_error(
                        rpc,
                        error,
                        completed,
                        format!("第 {} 项执行失败", index + 1),
                    )
                    .await;
            }
            completed = index + 1;
            if let Err(error) = mutation_request(
                rpc,
                &mut events,
                cancel,
                "EditSendProgress",
                json!({"Value": completed as f64 / total as f64}),
            )
            .await
            {
                return results
                    .transaction_error(
                        rpc,
                        error,
                        completed,
                        format!("第 {completed} 项后进度上报失败"),
                    )
                    .await;
            }
            self.update_editor_edit_progress(&operation_id, completed, total)
                .await;
        }
        if cancel.load(Ordering::SeqCst) {
            let rollback = rpc
                .request("EditEnd", json!({"Cancel": true}))
                .await
                .and_then(require_true);
            return results.result(
                if rollback.is_ok() {
                    EditorEditOutcome::CancelledRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if rollback.is_ok() {
                    "已取消并恢复编辑前状态。"
                } else {
                    "已请求取消，但无法确认恢复结果。"
                },
                completed,
                None,
            );
        }
        if let Err(error) = rpc
            .request("EditEnd", json!({"Cancel": false}))
            .await
            .and_then(require_true)
        {
            return results.result(
                EditorEditOutcome::Unknown,
                format!("无法确认 Editor 是否提交：{error}"),
                completed,
                None,
            );
        }
        let mut verification = EditorEditVerification {
            total,
            verified: 0,
            mismatched_indices: Vec::new(),
            unverifiable_indices: Vec::new(),
        };
        let ordered_positions = expected_ordered_move_positions(plan);
        let shared_verification = shared_verification_snapshot(rpc, &plan.method, &plan.model_uid)
            .await
            .ok()
            .flatten();
        for (index, item) in plan.items.iter().enumerate() {
            match verification_snapshot(rpc, &plan.method, &item.data).await {
                Ok(snapshot) => {
                    let mut expected = item.data.clone();
                    if let Some(position) = ordered_positions.as_ref().and_then(|positions| {
                        item.data
                            .get("Id")
                            .and_then(Value::as_str)
                            .and_then(|id| positions.get(id))
                    }) {
                        expected["InsertIndex"] = json!(position);
                    }
                    match verify_postcondition(
                        &plan.method,
                        &expected,
                        &snapshot,
                        shared_verification.as_ref(),
                    ) {
                        Some(true) => verification.verified += 1,
                        Some(false) => verification.mismatched_indices.push(index + 1),
                        None => verification.unverifiable_indices.push(index + 1),
                    }
                }
                Err(_) => verification.unverifiable_indices.push(index + 1),
            }
        }
        if verification.verified == total {
            results.result(
                EditorEditOutcome::Committed,
                format!("Editor 已提交，{total} 项修改均通过回读语义验证。"),
                completed,
                Some(verification),
            )
        } else {
            results.result(
                EditorEditOutcome::Unknown,
                "Editor 已结束事务，但整批回读未全部通过。",
                completed,
                Some(verification),
            )
        }
    }

    async fn update_editor_edit_progress(
        &self,
        operation_id: &str,
        completed: usize,
        total: usize,
    ) {
        if let Some(result) = self
            .inner
            .lock()
            .await
            .editor_edit_results
            .get_mut(operation_id)
        {
            result.completed = completed;
            result.total = total;
            result.message = format!("编辑事务正在执行：{completed}/{total}。");
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
            inner.apply_editor_edit_outcome(&result.outcome);
            insert_bounded_result(
                &mut inner.editor_edit_results,
                operation_id.into(),
                result.clone(),
            );
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
