mod agent;
mod domain;
mod protocol;
mod service;

use agent::{
    agent_answer_question, agent_cancel_turn, agent_create_conversation,
    agent_decide_computer_permission, agent_decide_plan, agent_delete_conversation,
    agent_discard_image_drafts, agent_discard_psd, agent_get_computer_permission,
    agent_get_messages, agent_get_pending_user_action, agent_get_plan, agent_list_conversations,
    agent_list_projects, agent_list_psds, agent_prepare_images, agent_prepare_psd,
    agent_send_message, agent_set_conversation_pinned, llm_get_config, llm_set_config,
    llm_test_connection, memory_list, memory_set_enabled, AgentRuntime, AgentStore,
};
use service::{
    cancel_parameter_batch, connect_editor, disconnect_editor, execute_parameter_batch,
    find_selected_part_parameters, get_editor_snapshot, preview_parameter_batch, EditorService,
};
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(EditorService::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_lilia::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            std::fs::create_dir_all(&dir)?;
            let store = AgentStore::default();
            store
                .open(dir.join("agent.db"))
                .map_err(|e| Box::<dyn std::error::Error>::from(e.message))?;
            store
                .clear_unresumable_pending_user_actions()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.message))?;
            if let Some(cache_dir) = store.cache_dir() {
                let computer_cache = cache_dir.join("computer-operation");
                if computer_cache.exists() {
                    std::fs::remove_dir_all(computer_cache)?;
                }
            }
            let coordinator = app.state::<EditorService>().operation_coordinator();
            let runtime = Arc::new(AgentRuntime::new(store, coordinator));
            runtime
                .images
                .clear_stale_drafts()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.message))?;
            app.manage(runtime);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect_editor,
            disconnect_editor,
            get_editor_snapshot,
            find_selected_part_parameters,
            preview_parameter_batch,
            execute_parameter_batch,
            cancel_parameter_batch,
            llm_get_config,
            llm_set_config,
            llm_test_connection,
            agent_list_conversations,
            agent_create_conversation,
            agent_set_conversation_pinned,
            agent_delete_conversation,
            agent_get_messages,
            agent_prepare_images,
            agent_discard_image_drafts,
            agent_prepare_psd,
            agent_discard_psd,
            agent_list_psds,
            agent_send_message,
            agent_cancel_turn,
            agent_answer_question,
            agent_decide_computer_permission,
            agent_get_computer_permission,
            agent_decide_plan,
            agent_get_plan,
            agent_get_pending_user_action,
            agent_list_projects,
            memory_list,
            memory_set_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_starts_hidden_at_minimum_size_until_plugin_restores_it() {
        for source in [
            include_str!("../tauri.conf.json"),
            include_str!("../tauri.macos.conf.json"),
        ] {
            let config: serde_json::Value = serde_json::from_str(source).unwrap();
            let main_window = config["app"]["windows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|window| window["label"].as_str() == Some("main"))
                .unwrap();

            assert_eq!(main_window["width"], main_window["minWidth"]);
            assert_eq!(main_window["height"], main_window["minHeight"]);
            assert_eq!(main_window["visible"].as_bool(), Some(false));
        }
    }

    #[test]
    fn tool_table_excludes_file_editing() {
        let tools = crate::agent::tools::all_tool_definitions().unwrap();
        let names: Vec<String> = tools
            .iter()
            .filter_map(|tool| {
                tool.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
            })
            .collect();
        for forbidden in ["read_file", "write_file", "apply_patch", "run_terminal"] {
            assert!(!names.iter().any(|name| name == forbidden));
        }
        assert!(names.iter().any(|name| name == "ask_user"));
        assert!(names.iter().any(|name| name == "update_plan"));
        assert!(names
            .iter()
            .any(|name| name == "capture_cubism_editor_window"));
    }
}
