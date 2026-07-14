use super::{CommandError, EditorService};
use crate::domain::{
    EditorSnapshot, OperationAccepted, ParameterBatchInput, ParameterBatchPreview,
    PartParameterQueryResult,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn connect_editor(
    port: u16,
    app: AppHandle,
    service: State<'_, EditorService>,
) -> Result<EditorSnapshot, CommandError> {
    service.start_connection(app, port).await
}

#[tauri::command]
pub async fn disconnect_editor(
    app: AppHandle,
    service: State<'_, EditorService>,
) -> Result<(), CommandError> {
    service.disconnect(&app).await
}

#[tauri::command]
pub async fn get_editor_snapshot(
    service: State<'_, EditorService>,
) -> Result<EditorSnapshot, CommandError> {
    Ok(service.snapshot().await)
}

#[tauri::command]
pub async fn find_selected_part_parameters(
    service: State<'_, EditorService>,
) -> Result<PartParameterQueryResult, CommandError> {
    service.find_part_parameters().await
}

#[tauri::command]
pub async fn preview_parameter_batch(
    input: ParameterBatchInput,
    service: State<'_, EditorService>,
) -> Result<ParameterBatchPreview, CommandError> {
    service.preview_batch(input).await
}

#[tauri::command]
pub async fn execute_parameter_batch(
    preview_id: String,
    app: AppHandle,
    service: State<'_, EditorService>,
) -> Result<OperationAccepted, CommandError> {
    service.execute_batch(app, preview_id).await
}

#[tauri::command]
pub async fn cancel_parameter_batch(
    operation_id: String,
    app: AppHandle,
    service: State<'_, EditorService>,
) -> Result<(), CommandError> {
    service.cancel_batch(&app, &operation_id).await
}
