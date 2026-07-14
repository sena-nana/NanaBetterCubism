use crate::domain::{BatchOutcome, BatchPhase, BatchProgress};
use crate::protocol::{RpcClient, RpcError, RpcEvent};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

use super::BATCH_PROGRESS_EVENT;

#[derive(Debug)]
pub(super) enum ExecutionError {
    AppCancelled,
    UserCancelled(bool),
    Rpc(RpcError),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AppCancelled => write!(formatter, "用户请求取消"),
            Self::UserCancelled(_) => write!(formatter, "用户在 Editor 中取消"),
            Self::Rpc(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<RpcError> for ExecutionError {
    fn from(value: RpcError) -> Self {
        Self::Rpc(value)
    }
}

pub(super) async fn mutation_request(
    rpc: &RpcClient,
    events: &mut broadcast::Receiver<RpcEvent>,
    cancel: &AtomicBool,
    method: &str,
    data: Value,
) -> Result<Value, ExecutionError> {
    if cancel.load(Ordering::SeqCst) {
        return Err(ExecutionError::AppCancelled);
    }
    let request = rpc.request(method, data);
    tokio::pin!(request);
    loop {
        tokio::select! {
            response = &mut request => return response.map_err(ExecutionError::Rpc),
            event = events.recv() => {
                match event {
                    Ok(event) if event.method == "NotifyUndoCancel" => {
                        return Err(ExecutionError::UserCancelled(
                            event.data.get("Result").and_then(Value::as_bool).unwrap_or(false),
                        ));
                    }
                    Ok(event) if event.method == "__Disconnected" => {
                        return Err(ExecutionError::Rpc(RpcError::Disconnected));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(ExecutionError::Rpc(RpcError::Disconnected));
                    }
                    _ => {}
                }
            }
        }
    }
}

pub(super) fn pre_begin_failure(error: ExecutionError) -> (BatchOutcome, String) {
    match error {
        ExecutionError::AppCancelled => (BatchOutcome::Failed, "操作在事务开始前已取消。".into()),
        ExecutionError::UserCancelled(result) if result => (
            BatchOutcome::CancelledRolledBack,
            "Editor 已取消操作。".into(),
        ),
        ExecutionError::UserCancelled(_) => (
            BatchOutcome::Unknown,
            "Editor 通知了取消，但未确认结果。".into(),
        ),
        error => (BatchOutcome::Failed, error.to_string()),
    }
}

pub(super) fn require_true(value: Value) -> Result<Value, RpcError> {
    if value.get("Result").and_then(Value::as_bool) == Some(true) {
        Ok(value)
    } else {
        Err(RpcError::Protocol("Editor 未确认操作成功".into()))
    }
}

pub(super) fn require_execution_true(value: Value) -> Result<Value, ExecutionError> {
    require_true(value).map_err(ExecutionError::Rpc)
}

pub(super) fn emit_progress(
    app: &AppHandle,
    operation_id: &str,
    phase: BatchPhase,
    completed: usize,
    total: usize,
    current_id: Option<String>,
) {
    let _ = app.emit(
        BATCH_PROGRESS_EVENT,
        BatchProgress {
            operation_id: operation_id.into(),
            phase,
            completed,
            total,
            current_id,
        },
    );
}
