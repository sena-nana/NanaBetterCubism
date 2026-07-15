mod agent;
mod domain;
mod protocol;
mod service;

use agent::{
    agent_answer_ask, agent_bind_project, agent_cancel_turn, agent_consolidate_memory,
    agent_archive_conversation, agent_create_conversation, agent_get_messages,
    agent_get_pending_ask, agent_get_plan, agent_list_conversations, agent_list_projects,
    agent_send_message, agent_set_conversation_pinned, agent_upsert_project,
    llm_get_config, llm_set_config, llm_test_connection, memory_list, memory_set_enabled,
    memory_upsert, AgentRuntime, AgentStore,
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
            app.manage(Arc::new(AgentRuntime::new(store)));
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
            agent_archive_conversation,
            agent_get_messages,
            agent_send_message,
            agent_cancel_turn,
            agent_answer_ask,
            agent_get_plan,
            agent_get_pending_ask,
            agent_list_projects,
            agent_upsert_project,
            agent_bind_project,
            memory_list,
            memory_upsert,
            memory_set_enabled,
            agent_consolidate_memory,
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

    #[test]
    fn tool_table_excludes_file_editing() {
        let tools = crate::agent::tools::tool_definitions();
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
