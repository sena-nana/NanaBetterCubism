mod domain;
mod protocol;
mod service;

use service::{
    cancel_parameter_batch, connect_editor, disconnect_editor, execute_parameter_batch,
    get_editor_snapshot, preview_parameter_batch, EditorService,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(EditorService::default())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_lilia::init())
        .invoke_handler(tauri::generate_handler![
            connect_editor,
            disconnect_editor,
            get_editor_snapshot,
            preview_parameter_batch,
            execute_parameter_batch,
            cancel_parameter_batch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_starts_hidden_until_plugin_restores_it() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main_window = config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["label"].as_str() == Some("main"))
            .unwrap();

        assert_eq!(main_window["visible"].as_bool(), Some(false));
    }
}
