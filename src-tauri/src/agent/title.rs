use crate::agent::llm::{chat_completions, content_to_text};
use crate::agent::{emit_conversations_changed, AgentRuntime};
use serde_json::json;
use std::sync::Arc;
use tauri::AppHandle;

const TITLE_PROMPT: &str =
    "根据用户第一条消息生成简短会话标题。只输出标题本身，不超过12个字，不要引号、标点或解释。";

pub async fn generate_conversation_title(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    conversation_id: String,
    user_text: String,
) {
    let Ok(config) = runtime.store.get_llm_config() else {
        return;
    };
    let input: String = user_text.chars().take(200).collect();
    let Ok(message) = chat_completions(
        &config,
        &[
            json!({ "role": "system", "content": TITLE_PROMPT }),
            json!({ "role": "user", "content": input }),
        ],
        &[],
    )
    .await
    else {
        return;
    };
    let Some(title) = sanitize_title(&content_to_text(&message.content)) else {
        return;
    };
    if runtime
        .store
        .set_conversation_title(&conversation_id, &title)
        .is_ok()
    {
        emit_conversations_changed(&app);
    }
}

pub(crate) fn sanitize_title(raw: &str) -> Option<String> {
    let cleaned = raw
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '「' | '」' | '“' | '”'));
    let title: String = cleaned.chars().take(12).collect::<String>().trim().to_string();
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_title_keeps_short_clean_names() {
        assert_eq!(
            sanitize_title("\"眼睛参数调整\"\n其他").as_deref(),
            Some("眼睛参数调整")
        );
        assert_eq!(sanitize_title("「嘴巴开合」").as_deref(), Some("嘴巴开合"));
        assert_eq!(sanitize_title("   "), None);
        assert_eq!(
            sanitize_title("这是一个非常非常非常长的会话标题内容").as_deref(),
            Some("这是一个非常非常非常长的")
        );
    }
}
