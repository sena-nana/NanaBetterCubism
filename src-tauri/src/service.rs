mod commands;
mod credentials;
mod model_structure;
pub(crate) mod official_api;
mod part_parameters;
mod transaction;

pub use commands::{
    cancel_parameter_batch, connect_editor, disconnect_editor, execute_parameter_batch,
    find_selected_part_parameters, get_editor_snapshot, preview_parameter_batch,
};

use crate::domain::{
    build_preview, BatchFinished, BatchOutcome, BatchPhase, EditorCapabilities,
    EditorConnectionState, EditorEditResult, EditorSnapshot, ModelStructure, OperationAccepted,
    ParameterBatchInput, ParameterBatchPreview, PartParameterQueryResult, StoredEditorEditPlan,
    StoredPlan, EDIT_API_VERSION,
};
use crate::protocol::{RpcClient, RpcError};
use credentials::{load_token, save_token};
#[cfg(test)]
use model_structure::parse_structure;
use model_structure::{fetch_structure, verify_plan};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, Mutex};
use transaction::{
    emit_progress, mutation_request, pre_begin_failure, require_execution_true, require_true,
    ExecutionError,
};
use uuid::Uuid;

const EDITOR_STATE_EVENT: &str = "cubism://editor-state";
const BATCH_PROGRESS_EVENT: &str = "cubism://parameter-batch-progress";
const BATCH_FINISHED_EVENT: &str = "cubism://parameter-batch-finished";
const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<RpcError> for CommandError {
    fn from(error: RpcError) -> Self {
        let code = match &error {
            RpcError::Connection(_) => "connection_failed",
            RpcError::Disconnected => "disconnected",
            RpcError::Timeout => "timeout",
            RpcError::Protocol(_) => "protocol_error",
            RpcError::Editor { kind, .. } => kind,
        };
        Self::new(code, error.to_string())
    }
}

#[derive(Clone)]
pub struct EditorService {
    inner: Arc<Mutex<ServiceState>>,
}

struct ActiveOperation {
    id: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct ServiceState {
    connection_request: u64,
    generation: u64,
    desired_connected: bool,
    snapshot: EditorSnapshot,
    rpc: Option<RpcClient>,
    model_uid: Option<String>,
    structure: ModelStructure,
    previews: HashMap<String, StoredPlan>,
    editor_edit_previews: HashMap<String, StoredEditorEditPlan>,
    editor_edit_results: HashMap<String, EditorEditResult>,
    document_refs: HashMap<String, String>,
    operation: Option<ActiveOperation>,
    part_query_in_progress: bool,
}

impl Default for EditorService {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ServiceState::default())),
        }
    }
}

impl EditorService {
    pub(crate) async fn snapshot(&self) -> EditorSnapshot {
        self.inner.lock().await.snapshot.clone()
    }

    async fn emit_snapshot(&self, app: &AppHandle) -> EditorSnapshot {
        let snapshot = self.snapshot().await;
        let _ = app.emit(EDITOR_STATE_EVENT, &snapshot);
        snapshot
    }

    async fn set_snapshot(
        &self,
        app: &AppHandle,
        state: EditorConnectionState,
        message: impl Into<String>,
        capabilities: bool,
    ) {
        {
            let mut inner = self.inner.lock().await;
            let official_api = capabilities
                || state == EditorConnectionState::AwaitingEditPermission
                || (state == EditorConnectionState::Incompatible
                    && inner.snapshot.capabilities.official_api);
            inner.snapshot.state = state;
            inner.snapshot.message = message.into();
            inner.snapshot.capabilities.batch_create_parameters = capabilities;
            inner.snapshot.capabilities.find_part_parameters = capabilities;
            inner.snapshot.capabilities.official_api = official_api;
            inner.snapshot.capabilities.official_edit_api = capabilities;
        }
        self.emit_snapshot(app).await;
    }

    async fn request_is_current(&self, request: u64) -> bool {
        let inner = self.inner.lock().await;
        inner.desired_connected && inner.connection_request == request
    }

    pub(crate) async fn start_connection(
        &self,
        app: AppHandle,
        port: u16,
    ) -> Result<EditorSnapshot, CommandError> {
        if port == 0 {
            return Err(CommandError::new(
                "invalid_port",
                "端口必须在 1 到 65535 之间。",
            ));
        }
        let previous = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "Editor 编辑事务进行中，不能重新连接。",
                ));
            }
            inner.connection_request = inner.connection_request.wrapping_add(1);
            inner.generation = inner.generation.wrapping_add(1);
            inner.desired_connected = true;
            inner.snapshot = EditorSnapshot {
                state: EditorConnectionState::Connecting,
                port,
                api_version: None,
                model_label: None,
                groups: Vec::new(),
                capabilities: EditorCapabilities {
                    batch_create_parameters: false,
                    find_part_parameters: false,
                    official_api: false,
                    official_edit_api: false,
                },
                message: "正在连接 Cubism Editor…".into(),
            };
            clear_session_data(&mut inner);
            let previous = inner.rpc.take();
            (inner.connection_request, previous)
        };
        if let Some(rpc) = previous.1 {
            rpc.close().await;
        }
        self.emit_snapshot(&app).await;
        let service = self.clone();
        tokio::spawn(async move {
            service.connection_loop(app, previous.0, port).await;
        });
        Ok(self.snapshot().await)
    }

    async fn wait_to_retry(&self, app: &AppHandle, request: u64) -> bool {
        {
            let mut inner = self.inner.lock().await;
            if !inner.desired_connected || inner.connection_request != request {
                return false;
            }
            clear_session_data(&mut inner);
            inner.snapshot.state = EditorConnectionState::Connecting;
            inner.snapshot.message = "Editor 暂未连接，正在自动重试…".into();
        }
        self.emit_snapshot(app).await;
        tokio::time::sleep(RECONNECT_INTERVAL).await;
        self.request_is_current(request).await
    }

    async fn connection_loop(&self, app: AppHandle, request: u64, port: u16) {
        loop {
            if !self.request_is_current(request).await {
                return;
            }
            let rpc = match RpcClient::connect(port).await {
                Ok(rpc) => rpc,
                Err(error) if error.is_transport_failure() => {
                    if !self.wait_to_retry(&app, request).await {
                        return;
                    }
                    continue;
                }
                Err(error) => {
                    self.set_snapshot(
                        &app,
                        EditorConnectionState::Failed,
                        error.to_string(),
                        false,
                    )
                    .await;
                    return;
                }
            };

            {
                let mut inner = self.inner.lock().await;
                if !inner.desired_connected || inner.connection_request != request {
                    drop(inner);
                    rpc.close().await;
                    return;
                }
                inner.generation = inner.generation.wrapping_add(1);
                inner.rpc = Some(rpc.clone());
                inner.previews.clear();
                inner.part_query_in_progress = false;
            }

            match self.run_session(&app, request, rpc.clone()).await {
                Ok(()) => return,
                Err(error) => {
                    rpc.close().await;
                    {
                        let mut inner = self.inner.lock().await;
                        if inner.connection_request == request {
                            inner.rpc = None;
                            clear_session_data(&mut inner);
                        }
                    }
                    if !self.request_is_current(request).await {
                        return;
                    }
                    if error.is_transport_failure() {
                        if !self.wait_to_retry(&app, request).await {
                            return;
                        }
                        continue;
                    }
                    self.set_snapshot(
                        &app,
                        EditorConnectionState::Failed,
                        error.to_string(),
                        false,
                    )
                    .await;
                    return;
                }
            }
        }
    }

    async fn run_session(
        &self,
        app: &AppHandle,
        request: u64,
        rpc: RpcClient,
    ) -> Result<(), RpcError> {
        let saved_token = load_token();
        let registration = rpc
            .request(
                "RegisterPlugin",
                json!({
                    "Name": "NanaBetterCubism",
                    "Token": saved_token,
                }),
            )
            .await;
        let registration = match registration {
            Err(error)
                if matches!(
                    error.editor_kind(),
                    Some("UnsupportedVersion" | "MethodNotFound")
                ) =>
            {
                self.set_snapshot(
                    app,
                    EditorConnectionState::Incompatible,
                    "已连接，但当前 Editor 不支持 External API 1.1.0。",
                    false,
                )
                .await;
                return self.wait_for_disconnect(request, rpc).await;
            }
            value => value?,
        };
        {
            let mut inner = self.inner.lock().await;
            if inner.connection_request == request {
                inner.snapshot.api_version = Some(EDIT_API_VERSION.into());
            }
        }
        let returned_token = registration
            .get("Token")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::Protocol("RegisterPlugin 未返回令牌".into()))?
            .to_string();
        if returned_token != saved_token {
            save_token(&returned_token);
            self.set_snapshot(
                app,
                EditorConnectionState::AwaitingAccess,
                "请在 Editor 的外部应用联动设置中允许 NanaBetterCubism。",
                false,
            )
            .await;
        }

        let mut version_selected = false;
        loop {
            if !self.request_is_current(request).await {
                return Ok(());
            }
            let editing = {
                let inner = self.inner.lock().await;
                inner.operation.is_some()
            };
            if editing {
                tokio::time::sleep(Duration::from_millis(400)).await;
                continue;
            }

            if !response_bool(rpc.request("GetIsApproval", json!({})).await?)? {
                self.set_snapshot(
                    app,
                    EditorConnectionState::AwaitingAccess,
                    "请在 Editor 中允许 NanaBetterCubism 访问。",
                    false,
                )
                .await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            {
                let mut inner = self.inner.lock().await;
                if inner.connection_request == request {
                    inner.snapshot.capabilities.official_api = true;
                }
            }
            if !version_selected {
                rpc.request("SetGlobalVersion", json!({ "Version": EDIT_API_VERSION }))
                    .await?;
                version_selected = true;
            }

            let edit_approval = rpc.request("GetIsEditApproval", json!({})).await;
            let edit_approval = match edit_approval {
                Err(error)
                    if matches!(
                        error.editor_kind(),
                        Some("UnsupportedVersion" | "MethodNotFound")
                    ) =>
                {
                    self.set_snapshot(
                        app,
                        EditorConnectionState::Incompatible,
                        "已连接，但当前 Editor 不支持参数编辑 API 1.1.0。",
                        false,
                    )
                    .await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                value => value?,
            };
            if !response_bool(edit_approval)? {
                self.set_snapshot(
                    app,
                    EditorConnectionState::AwaitingEditPermission,
                    "请在 Editor 中启用 NanaBetterCubism 的“编辑”权限。",
                    false,
                )
                .await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let model = match rpc.request("GetCurrentModelUID", json!({})).await {
                Err(error) if error.editor_kind() == Some("InvalidModel") => {
                    self.set_snapshot(
                        app,
                        EditorConnectionState::Incompatible,
                        "请在 Editor 中打开并选中一个模型。",
                        false,
                    )
                    .await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                value => value?,
            };
            let model_uid = model
                .get("ModelUID")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::Protocol("GetCurrentModelUID 缺少 ModelUID".into()))?
                .to_string();
            let structure = match fetch_structure(&rpc, &model_uid).await {
                Err(error) if error.editor_kind() == Some("InvalidEditOperation") => {
                    self.set_snapshot(
                        app,
                        EditorConnectionState::Incompatible,
                        "已连接；参数编辑仅在 Cubism Editor 建模模式下可用。",
                        false,
                    )
                    .await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                Err(error) if error.editor_kind() == Some("InvalidModel") => {
                    self.set_snapshot(
                        app,
                        EditorConnectionState::Incompatible,
                        "请在 Editor 建模模式中打开并选中一个模型。",
                        false,
                    )
                    .await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                Err(error) if error.editor_kind() == Some("UnsupportedVersion") => {
                    self.set_snapshot(
                        app,
                        EditorConnectionState::Incompatible,
                        "已连接，但当前 Editor 不支持参数结构 API 1.1.0。",
                        false,
                    )
                    .await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                value => value?,
            };

            {
                let mut inner = self.inner.lock().await;
                if inner.connection_request != request || !inner.desired_connected {
                    return Ok(());
                }
                let changed = inner.model_uid.as_deref() != Some(&model_uid)
                    || inner.structure.semantic_hash() != structure.semantic_hash();
                if changed {
                    inner.previews.clear();
                }
                inner.model_uid = Some(model_uid);
                inner.structure = structure.clone();
                inner.snapshot.state = EditorConnectionState::Ready;
                inner.snapshot.api_version = Some(EDIT_API_VERSION.into());
                inner.snapshot.model_label = Some("当前建模模型".into());
                inner.snapshot.groups = structure.groups.clone();
                inner.snapshot.capabilities.batch_create_parameters = true;
                inner.snapshot.capabilities.find_part_parameters = true;
                inner.snapshot.capabilities.official_api = true;
                inner.snapshot.capabilities.official_edit_api = true;
                inner.snapshot.message = "已连接，可以使用 Editor 工具。".into();
            }
            self.emit_snapshot(app).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn wait_for_disconnect(&self, request: u64, rpc: RpcClient) -> Result<(), RpcError> {
        let mut events = rpc.subscribe();
        loop {
            if !self.request_is_current(request).await {
                return Ok(());
            }
            match tokio::time::timeout(Duration::from_secs(1), events.recv()).await {
                Ok(Ok(event)) if event.method == "__Disconnected" => {
                    return Err(RpcError::Disconnected)
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => return Err(RpcError::Disconnected),
                _ => {}
            }
        }
    }

    pub(crate) async fn disconnect(&self, app: &AppHandle) -> Result<(), CommandError> {
        let rpc = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "请先取消并等待 Editor 编辑事务结束。",
                ));
            }
            inner.desired_connected = false;
            inner.connection_request = inner.connection_request.wrapping_add(1);
            inner.generation = inner.generation.wrapping_add(1);
            clear_session_data(&mut inner);
            inner.snapshot = EditorSnapshot::default();
            inner.rpc.take()
        };
        if let Some(rpc) = rpc {
            rpc.close().await;
        }
        self.emit_snapshot(app).await;
        Ok(())
    }

    pub(crate) async fn preview_batch(
        &self,
        input: ParameterBatchInput,
    ) -> Result<ParameterBatchPreview, CommandError> {
        let (rpc, model_uid, generation, model_label) = {
            let inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "已有 Editor 编辑事务正在执行。",
                ));
            }
            if inner.snapshot.state != EditorConnectionState::Ready
                || !inner.snapshot.capabilities.batch_create_parameters
            {
                return Err(CommandError::new(
                    "editor_not_ready",
                    inner.snapshot.message.clone(),
                ));
            }
            (
                inner
                    .rpc
                    .clone()
                    .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?,
                inner
                    .model_uid
                    .clone()
                    .ok_or_else(|| CommandError::new("missing_model", "当前没有可编辑模型。"))?,
                inner.generation,
                inner
                    .snapshot
                    .model_label
                    .clone()
                    .unwrap_or_else(|| "当前建模模型".into()),
            )
        };
        let structure = fetch_structure(&rpc, &model_uid)
            .await
            .map_err(CommandError::from)?;
        let mut preview = build_preview(&input, &structure, &model_label);
        if preview.can_execute {
            let preview_id = Uuid::new_v4().simple().to_string();
            preview.preview_id = Some(preview_id.clone());
            let plan = StoredPlan {
                preview_id: preview_id.clone(),
                generation,
                model_uid: model_uid.clone(),
                structure_hash: structure.semantic_hash(),
                new_group: preview.new_group.clone(),
                rows: preview.rows.clone(),
            };
            let mut inner = self.inner.lock().await;
            if inner.generation != generation || inner.model_uid.as_deref() != Some(&model_uid) {
                return Err(CommandError::new(
                    "stale_model",
                    "预览期间当前模型已变化，请重新校验。",
                ));
            }
            inner.structure = structure;
            inner.previews.clear();
            inner.previews.insert(preview_id, plan);
        }
        Ok(preview)
    }

    pub(crate) async fn find_part_parameters(
        &self,
    ) -> Result<PartParameterQueryResult, CommandError> {
        let (rpc, model_uid, generation, model_label) = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "Editor 编辑事务进行中，暂时不能查询部件关联。",
                ));
            }
            if inner.part_query_in_progress {
                return Err(CommandError::new(
                    "query_active",
                    "已有部件关联查询正在进行。",
                ));
            }
            if inner.snapshot.state != EditorConnectionState::Ready
                || !inner.snapshot.capabilities.find_part_parameters
            {
                return Err(CommandError::new(
                    "editor_not_ready",
                    inner.snapshot.message.clone(),
                ));
            }
            let rpc = inner
                .rpc
                .clone()
                .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?;
            let model_uid = inner
                .model_uid
                .clone()
                .ok_or_else(|| CommandError::new("missing_model", "当前没有可编辑模型。"))?;
            inner.part_query_in_progress = true;
            (
                rpc,
                model_uid,
                inner.generation,
                inner
                    .snapshot
                    .model_label
                    .clone()
                    .unwrap_or_else(|| "当前建模模型".into()),
            )
        };

        let result = part_parameters::find_selected(&rpc, &model_uid, &model_label).await;
        let current = {
            let mut inner = self.inner.lock().await;
            let current = inner.generation == generation
                && inner.model_uid.as_deref() == Some(&model_uid)
                && inner.snapshot.state == EditorConnectionState::Ready
                && inner.operation.is_none();
            if inner.generation == generation {
                inner.part_query_in_progress = false;
            }
            current
        };
        if !current {
            return Err(CommandError::new(
                "stale_query",
                "连接或模型在查询期间发生变化，请重试。",
            ));
        }
        result
    }

    pub(crate) async fn execute_batch(
        &self,
        app: AppHandle,
        preview_id: String,
    ) -> Result<OperationAccepted, CommandError> {
        let (operation_id, plan, rpc, cancel) = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "已有 Editor 编辑事务正在执行。",
                ));
            }
            if inner.part_query_in_progress {
                return Err(CommandError::new(
                    "query_active",
                    "部件关联查询进行中，请等待查询结束后再创建参数。",
                ));
            }
            let plan = inner
                .previews
                .remove(&preview_id)
                .ok_or_else(|| CommandError::new("stale_preview", "预览已失效，请重新校验。"))?;
            if plan.preview_id != preview_id
                || plan.generation != inner.generation
                || inner.model_uid.as_deref() != Some(&plan.model_uid)
            {
                return Err(CommandError::new(
                    "stale_preview",
                    "模型或连接已变化，请重新校验。",
                ));
            }
            let rpc = inner
                .rpc
                .clone()
                .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?;
            let operation_id = Uuid::new_v4().simple().to_string();
            let cancel = Arc::new(AtomicBool::new(false));
            inner.operation = Some(ActiveOperation {
                id: operation_id.clone(),
                cancel: cancel.clone(),
            });
            inner.snapshot.state = EditorConnectionState::Editing;
            inner.snapshot.capabilities.batch_create_parameters = false;
            inner.snapshot.capabilities.find_part_parameters = false;
            inner.snapshot.capabilities.official_edit_api = false;
            inner.snapshot.message = format!("正在创建 {} 个参数…", plan.rows.len());
            (operation_id, plan, rpc, cancel)
        };
        self.emit_snapshot(&app).await;
        let service = self.clone();
        let accepted = OperationAccepted {
            operation_id: operation_id.clone(),
        };
        tokio::spawn(async move {
            service
                .run_batch_operation(app, operation_id, plan, rpc, cancel)
                .await;
        });
        Ok(accepted)
    }

    pub(crate) async fn cancel_batch(
        &self,
        app: &AppHandle,
        operation_id: &str,
    ) -> Result<(), CommandError> {
        {
            let mut inner = self.inner.lock().await;
            let operation = inner.operation.as_ref().ok_or_else(|| {
                CommandError::new("missing_operation", "当前没有 Editor 编辑事务。")
            })?;
            if operation.id != operation_id {
                return Err(CommandError::new("stale_operation", "操作 ID 已失效。"));
            }
            operation.cancel.store(true, Ordering::SeqCst);
            inner.snapshot.state = EditorConnectionState::Cancelling;
            inner.snapshot.message = "正在请求 Editor 取消并恢复事务…".into();
        }
        self.emit_snapshot(app).await;
        Ok(())
    }

    async fn run_batch_operation(
        &self,
        app: AppHandle,
        operation_id: String,
        plan: StoredPlan,
        rpc: RpcClient,
        cancel: Arc<AtomicBool>,
    ) {
        let total = plan.rows.len();
        emit_progress(&app, &operation_id, BatchPhase::Validating, 0, total, None);
        let current_model = rpc.request("GetCurrentModelUID", json!({})).await;
        let current_model = match current_model {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Failed,
                    vec![],
                    error.to_string(),
                    None,
                )
                .await;
                return;
            }
        };
        if current_model.get("ModelUID").and_then(Value::as_str) != Some(&plan.model_uid) {
            self.finish_operation(
                &app,
                &operation_id,
                BatchOutcome::Failed,
                vec![],
                "当前模型已变化，请重新预览。".into(),
                None,
            )
            .await;
            return;
        }
        let structure = match fetch_structure(&rpc, &plan.model_uid).await {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Failed,
                    vec![],
                    error.to_string(),
                    None,
                )
                .await;
                return;
            }
        };
        if structure.semantic_hash() != plan.structure_hash {
            self.finish_operation(
                &app,
                &operation_id,
                BatchOutcome::Failed,
                vec![],
                "模型参数结构已变化，请重新预览。".into(),
                Some(structure),
            )
            .await;
            return;
        }

        let mut events = rpc.subscribe();
        let cancel_notifications = rpc
            .request("NotifyUndoCancel", json!({ "Enabled": true }))
            .await;
        match cancel_notifications {
            Ok(value) if value.get("Accepted").and_then(Value::as_bool) == Some(true) => {}
            Ok(_) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Failed,
                    vec![],
                    "Editor 未接受取消通知订阅，未开始编辑。".into(),
                    None,
                )
                .await;
                return;
            }
            Err(error) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Failed,
                    vec![],
                    format!("无法启用 Editor 取消通知，未开始编辑：{error}"),
                    None,
                )
                .await;
                return;
            }
        }
        emit_progress(&app, &operation_id, BatchPhase::Beginning, 0, total, None);
        let begin = mutation_request(
            &rpc,
            &mut events,
            &cancel,
            "EditBegin",
            json!({ "Silent": false }),
        )
        .await
        .and_then(require_execution_true);
        if let Err(error) = begin {
            let (outcome, message) = pre_begin_failure(error);
            self.finish_operation(&app, &operation_id, outcome, vec![], message, None)
                .await;
            return;
        }
        let mut created_ids = Vec::new();

        if let Some(group) = &plan.new_group {
            emit_progress(
                &app,
                &operation_id,
                BatchPhase::CreatingGroup,
                0,
                total,
                Some(group.id.clone()),
            );
            let result = mutation_request(
                &rpc,
                &mut events,
                &cancel,
                "AddParameterGroup",
                json!({ "ModelUID": plan.model_uid, "Name": group.name, "Id": group.id }),
            )
            .await
            .and_then(require_execution_true);
            if let Err(error) = result {
                self.finish_after_edit_error(&app, &operation_id, &rpc, error, created_ids)
                    .await;
                return;
            }
        }

        for (index, row) in plan.rows.iter().enumerate() {
            emit_progress(
                &app,
                &operation_id,
                BatchPhase::CreatingParameters,
                index,
                total,
                Some(row.id.clone()),
            );
            let mut data = json!({
                "ModelUID": plan.model_uid,
                "Name": row.name,
                "Id": row.id,
                "Min": row.min,
                "Default": row.default,
                "Max": row.max,
                "IsBlendShape": row.is_blend_shape,
            });
            if let Some(group_id) = &row.group_id {
                data["GroupId"] = json!(group_id);
            }
            let added = mutation_request(&rpc, &mut events, &cancel, "AddParameter", data)
                .await
                .and_then(require_execution_true);
            if let Err(error) = added {
                self.finish_after_edit_error(&app, &operation_id, &rpc, error, created_ids)
                    .await;
                return;
            }
            created_ids.push(row.id.clone());

            if row.is_repeat {
                let repeated = mutation_request(
                    &rpc,
                    &mut events,
                    &cancel,
                    "EditParameter",
                    json!({ "ModelUID": plan.model_uid, "Id": row.id, "IsRepeat": true }),
                )
                .await
                .and_then(require_execution_true);
                if let Err(error) = repeated {
                    self.finish_after_edit_error(&app, &operation_id, &rpc, error, created_ids)
                        .await;
                    return;
                }
            }

            let progress = (index + 1) as f64 / total as f64;
            let sent_progress = mutation_request(
                &rpc,
                &mut events,
                &cancel,
                "EditSendProgress",
                json!({ "Value": progress }),
            )
            .await;
            if let Err(error) = sent_progress {
                self.finish_after_edit_error(&app, &operation_id, &rpc, error, created_ids)
                    .await;
                return;
            }
            emit_progress(
                &app,
                &operation_id,
                BatchPhase::CreatingParameters,
                index + 1,
                total,
                Some(row.id.clone()),
            );
        }

        emit_progress(
            &app,
            &operation_id,
            BatchPhase::Committing,
            total,
            total,
            None,
        );
        if cancel.load(Ordering::SeqCst) {
            self.finish_after_edit_error(
                &app,
                &operation_id,
                &rpc,
                ExecutionError::AppCancelled,
                created_ids,
            )
            .await;
            return;
        }
        let committed = rpc
            .request("EditEnd", json!({ "Cancel": false }))
            .await
            .and_then(require_true);
        if let Err(error) = committed {
            self.finish_operation(
                &app,
                &operation_id,
                BatchOutcome::Unknown,
                vec![],
                format!("无法确认 Editor 是否提交：{error}"),
                None,
            )
            .await;
            return;
        }

        emit_progress(
            &app,
            &operation_id,
            BatchPhase::Verifying,
            total,
            total,
            None,
        );
        match fetch_structure(&rpc, &plan.model_uid).await {
            Ok(structure) if verify_plan(&plan, &structure) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Committed,
                    created_ids,
                    format!("已创建 {total} 个参数。"),
                    Some(structure),
                )
                .await;
            }
            Ok(structure) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Unknown,
                    vec![],
                    "Editor 已结束事务，但回读结果与预览不一致。".into(),
                    Some(structure),
                )
                .await;
            }
            Err(error) => {
                self.finish_operation(
                    &app,
                    &operation_id,
                    BatchOutcome::Unknown,
                    vec![],
                    format!("Editor 已结束事务，但无法回读验证：{error}"),
                    None,
                )
                .await;
            }
        }
    }

    async fn finish_after_edit_error(
        &self,
        app: &AppHandle,
        operation_id: &str,
        rpc: &RpcClient,
        error: ExecutionError,
        created_ids: Vec<String>,
    ) {
        if let ExecutionError::UserCancelled(result) = error {
            let outcome = if result {
                BatchOutcome::CancelledRolledBack
            } else {
                BatchOutcome::Unknown
            };
            let message = if result {
                "已由 Editor 取消并恢复编辑前状态。"
            } else {
                "Editor 通知了取消，但未确认恢复结果。"
            };
            self.finish_operation(app, operation_id, outcome, vec![], message.into(), None)
                .await;
            return;
        }
        if matches!(error, ExecutionError::Rpc(ref rpc_error) if rpc_error.is_transport_failure()) {
            self.finish_operation(
                app,
                operation_id,
                BatchOutcome::Unknown,
                vec![],
                format!("事务结果未知：{error}"),
                None,
            )
            .await;
            return;
        }

        emit_progress(
            app,
            operation_id,
            BatchPhase::Cancelling,
            0,
            created_ids.len(),
            None,
        );
        let rollback = rpc
            .request("EditEnd", json!({ "Cancel": true }))
            .await
            .and_then(require_true);
        match rollback {
            Ok(_) => {
                let outcome = if matches!(error, ExecutionError::AppCancelled) {
                    BatchOutcome::CancelledRolledBack
                } else {
                    BatchOutcome::FailedRolledBack
                };
                self.finish_operation(
                    app,
                    operation_id,
                    outcome,
                    vec![],
                    format!("Editor 已恢复编辑前状态：{error}"),
                    None,
                )
                .await;
            }
            Err(rollback_error) => {
                self.finish_operation(
                    app,
                    operation_id,
                    BatchOutcome::Unknown,
                    vec![],
                    format!("操作失败且无法确认回滚：{error}；{rollback_error}"),
                    None,
                )
                .await;
            }
        }
    }

    async fn finish_operation(
        &self,
        app: &AppHandle,
        operation_id: &str,
        outcome: BatchOutcome,
        created_ids: Vec<String>,
        message: String,
        structure: Option<ModelStructure>,
    ) {
        {
            let mut inner = self.inner.lock().await;
            if inner
                .operation
                .as_ref()
                .map(|operation| operation.id.as_str())
                != Some(operation_id)
            {
                return;
            }
            inner.operation = None;
            inner.previews.clear();
            if let Some(structure) = structure {
                inner.snapshot.groups = structure.groups.clone();
                inner.structure = structure;
            }
            let safe_to_continue = !matches!(outcome, BatchOutcome::Unknown);
            inner.snapshot.state = if safe_to_continue {
                EditorConnectionState::Ready
            } else {
                EditorConnectionState::Failed
            };
            inner.snapshot.capabilities.batch_create_parameters = safe_to_continue;
            inner.snapshot.capabilities.find_part_parameters = safe_to_continue;
            inner.snapshot.capabilities.official_api = safe_to_continue;
            inner.snapshot.capabilities.official_edit_api = safe_to_continue;
            inner.snapshot.message = message.clone();
        }
        let finished = BatchFinished {
            operation_id: operation_id.into(),
            outcome,
            created_ids,
            message,
        };
        let _ = app.emit(BATCH_FINISHED_EVENT, &finished);
        self.emit_snapshot(app).await;
    }
}

fn clear_session_data(inner: &mut ServiceState) {
    inner.model_uid = None;
    inner.structure = ModelStructure::default();
    inner.previews.clear();
    inner.editor_edit_previews.clear();
    inner.document_refs.clear();
    inner.part_query_in_progress = false;
    inner.snapshot.api_version = None;
    inner.snapshot.model_label = None;
    inner.snapshot.groups.clear();
    inner.snapshot.capabilities.batch_create_parameters = false;
    inner.snapshot.capabilities.find_part_parameters = false;
    inner.snapshot.capabilities.official_api = false;
    inner.snapshot.capabilities.official_edit_api = false;
}

fn response_bool(value: Value) -> Result<bool, RpcError> {
    value
        .get("Result")
        .and_then(Value::as_bool)
        .ok_or_else(|| RpcError::Protocol("响应缺少 Result".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ExistingParameter, ParameterPreviewRow};
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    async fn event_server(event: Option<Value>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let _ = socket.next().await;
            if let Some(event) = event {
                socket
                    .send(Message::Text(event.to_string().into()))
                    .await
                    .unwrap();
            } else {
                socket.close(None).await.unwrap();
            }
        });
        port
    }

    #[test]
    fn parses_root_and_grouped_parameters() {
        let structure = parse_structure(&json!({
            "ParameterStructure": {
                "Entries": [
                    {
                        "EntryType": "ParameterGroup",
                        "Id": "ParamGroupFace",
                        "Name": "Face",
                        "Parameters": [{
                            "EntryType": "Parameter",
                            "Id": "ParamEyeLOpen",
                            "Name": "Eye L",
                            "Min": 0,
                            "Default": 1,
                            "Max": 1,
                            "IsRepeat": false,
                            "IsBlendShape": false
                        }]
                    },
                    {
                        "EntryType": "Parameter",
                        "Id": "ParamAngleX",
                        "Name": "Angle X",
                        "Min": -30,
                        "Default": 0,
                        "Max": 30,
                        "IsRepeat": false,
                        "IsBlendShape": false
                    }
                ]
            }
        }))
        .unwrap();
        assert_eq!(structure.groups.len(), 1);
        assert_eq!(structure.parameters.len(), 2);
        assert_eq!(
            structure.parameters[0].group_id.as_deref(),
            Some("ParamGroupFace")
        );
        assert_eq!(structure.parameters[1].group_id, None);
    }

    #[test]
    fn verifies_semantic_parameter_postconditions() {
        let row = ParameterPreviewRow {
            client_id: "row".into(),
            name: "Hair".into(),
            id: "ParamHair01".into(),
            group_id: None,
            group_label: "根级".into(),
            min: -1.0,
            default: 0.0,
            max: 1.0,
            is_blend_shape: false,
            is_repeat: true,
        };
        let plan = StoredPlan {
            preview_id: "preview".into(),
            generation: 1,
            model_uid: "private".into(),
            structure_hash: String::new(),
            new_group: None,
            rows: vec![row.clone()],
        };
        let structure = ModelStructure {
            parameters: vec![ExistingParameter {
                id: row.id,
                name: row.name,
                group_id: None,
                min: -1.0,
                default: 0.0,
                max: 1.0,
                is_blend_shape: false,
                is_repeat: true,
            }],
            ..Default::default()
        };
        assert!(verify_plan(&plan, &structure));
    }

    #[tokio::test]
    async fn mutation_observes_editor_side_undo_cancel() {
        let port = event_server(Some(json!({
            "Version": EDIT_API_VERSION,
            "Type": "Event",
            "Method": "NotifyUndoCancel",
            "Data": { "Result": true }
        })))
        .await;
        let rpc = RpcClient::connect(port).await.unwrap();
        let mut events = rpc.subscribe();
        let cancel = AtomicBool::new(false);

        let result = mutation_request(&rpc, &mut events, &cancel, "AddParameter", json!({})).await;

        assert!(matches!(result, Err(ExecutionError::UserCancelled(true))));
    }

    #[tokio::test]
    async fn mutation_treats_disconnect_as_uncertain() {
        let port = event_server(None).await;
        let rpc = RpcClient::connect(port).await.unwrap();
        let mut events = rpc.subscribe();
        let cancel = AtomicBool::new(false);

        let result = mutation_request(&rpc, &mut events, &cancel, "AddParameter", json!({})).await;

        assert!(matches!(
            result,
            Err(ExecutionError::Rpc(RpcError::Disconnected))
        ));
    }

    #[tokio::test]
    async fn application_cancel_stops_before_next_mutation() {
        let port = event_server(None).await;
        let rpc = RpcClient::connect(port).await.unwrap();
        let mut events = rpc.subscribe();
        let cancel = AtomicBool::new(true);

        let result = mutation_request(&rpc, &mut events, &cancel, "AddParameter", json!({})).await;

        assert!(matches!(result, Err(ExecutionError::AppCancelled)));
    }
}
