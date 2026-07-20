use crate::agent::llm::{chat_completions, content_to_text};
use crate::agent::store::LlmConfigInternal;
use crate::agent::AgentError;
use serde_json::{json, Value};
use std::collections::HashMap;

/// 保守的文本 token 估算：约 3 字符 / token。
const TEXT_CHARS_PER_TOKEN: usize = 3;
/// 单张图片的最低 token 估算下限。
const IMAGE_TOKEN_FLOOR: usize = 1500;
/// base64 长度折算 token 的除数。
const IMAGE_BASE64_DIVISOR: usize = 100;
/// 工具结果裁剪时保留的近期 tool 消息条数。
const KEEP_RECENT_TOOL_RESULTS: usize = 4;
/// 截图占位裁剪时保留的近期含图消息条数。
const KEEP_RECENT_IMAGES: usize = 2;
/// 摘要时保留的尾部消息条数。
const KEEP_RECENT_TAIL: usize = 6;

/// 未配置上下文窗口时的默认值（token）。
const DEFAULT_CONTEXT_WINDOW: u32 = 256_000;

/// 解析输入 token 预算：优先 max_input_tokens，否则按 context_window 的 70% 估算，
/// context_window 未配置时按默认 256k 兜底。
pub fn resolve_budget(config: &LlmConfigInternal) -> Option<usize> {
    if let Some(max_input) = config.max_input_tokens {
        return Some(max_input as usize);
    }
    let window = config.context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);
    Some((window as usize * 7) / 10)
}

/// 估算消息序列 + 工具定义的 token 数。保守偏高，宁可早压缩。
pub fn estimate_tokens(messages: &[Value], tools: &[Value]) -> usize {
    let mut total = 0usize;
    for message in messages {
        total += estimate_message_tokens(message);
        total += 4; // 每条消息的包装开销
    }
    if !tools.is_empty() {
        let serialized = serde_json::to_string(tools).unwrap_or_default();
        total += serialized.len() / TEXT_CHARS_PER_TOKEN;
    }
    total
}

fn estimate_message_tokens(message: &Value) -> usize {
    let content = message.get("content");
    let mut tokens = 0usize;
    match content {
        Some(Value::String(text)) => tokens += text.len() / TEXT_CHARS_PER_TOKEN,
        Some(Value::Array(parts)) => {
            for part in parts {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                if kind == "image_url" {
                    let url = part
                        .get("image_url")
                        .and_then(|v| v.get("url"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    tokens += image_tokens(url);
                } else if let Some(text) = part.get("text").and_then(Value::as_str) {
                    tokens += text.len() / TEXT_CHARS_PER_TOKEN;
                }
            }
        }
        _ => {}
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
            {
                tokens += args.len() / TEXT_CHARS_PER_TOKEN;
            }
            tokens += 8;
        }
    }
    tokens
}

fn image_tokens(data_url: &str) -> usize {
    let len = data_url.len();
    let estimated = len / IMAGE_BASE64_DIVISOR;
    if estimated > IMAGE_TOKEN_FLOOR {
        estimated
    } else {
        IMAGE_TOKEN_FLOOR
    }
}

/// 构建 tool_call_id -> function.name 的索引，用于识别 read_skill 等不可裁剪的工具结果。
fn build_tool_call_index(messages: &[Value]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
            continue;
        };
        for call in calls {
            let Some(id) = call.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            map.insert(id.to_string(), name.to_string());
        }
    }
    map
}

/// 裁剪旧的工具结果：保留最近 `keep_recent` 条可裁剪 tool 消息，更早的替换为短占位。
/// read_skill 的工具结果始终保留，避免模型在回合中途丢失技能指令。
/// 结构保持合法：每个 assistant tool_call 仍对应一条 tool 消息，只替换内容不删除。
pub fn trim_tool_results(messages: &mut Vec<Value>, keep_recent: usize) {
    let index = build_tool_call_index(messages);
    let read_skill = crate::agent::skills::READ_SKILL_TOOL_NAME;

    let mut trimmable: Vec<usize> = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        if message.get("role").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        let tool_call_id = message.get("tool_call_id").and_then(Value::as_str).unwrap_or("");
        let name = index.get(tool_call_id).map(String::as_str).unwrap_or("");
        if name == read_skill {
            continue;
        }
        trimmable.push(idx);
    }

    let cutoff = trimmable.len().saturating_sub(keep_recent);
    for &idx in trimmable.iter().take(cutoff) {
        let original = messages[idx]
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .len();
        let placeholder = format!("[tool result trimmed: {original} chars]");
        if let Some(obj) = messages[idx].as_object_mut() {
            obj.insert("content".into(), Value::String(placeholder));
        }
    }
}

/// 裁剪旧消息中的图片：保留最近 `keep_recent` 条含 image_url 的消息，更早的把图片片段降级为文本占位。
pub fn trim_old_images(messages: &mut Vec<Value>, keep_recent: usize) {
    let mut image_indices: Vec<usize> = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        let Some(parts) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        if parts
            .iter()
            .any(|part| part.get("type").and_then(Value::as_str) == Some("image_url"))
        {
            image_indices.push(idx);
        }
    }

    let cutoff = image_indices.len().saturating_sub(keep_recent);
    for &idx in image_indices.iter().take(cutoff) {
        let Some(parts) = messages[idx].get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut joined_text = String::new();
        for part in parts.iter_mut() {
            if part.get("type").and_then(Value::as_str) == Some("image_url") {
                *part = json!({
                    "type": "text",
                    "text": "[图片已隐藏：超出上下文预算，早期截图已压缩]"
                });
            }
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                joined_text.push_str(text);
            }
        }
        if !joined_text.is_empty() {
            messages[idx]["content"] = Value::String(joined_text);
        } else {
            messages[idx]["content"] =
                Value::String("[图片已隐藏：超出上下文预算，早期截图已压缩]".into());
        }
    }
}

/// 统计前导系统消息条数（head），这些消息在摘要时原样保留。
fn count_head_system_messages(messages: &[Value]) -> usize {
    messages
        .iter()
        .take_while(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .count()
}

/// 将一条消息渲染为摘要输入用的纯文本行。
fn render_message_for_summary(message: &Value) -> String {
    let role = message.get("role").and_then(Value::as_str).unwrap_or("?");
    let content = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    let mut line = format!("[{role}] {content}");
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let args = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or("");
            line.push_str(&format!("\n  -> call {name}: {args}"));
        }
    }
    line
}

/// 对早期多轮消息做一次 LLM 摘要，用单条 system 摘要消息替换被摘要的前缀。
/// 保留前导系统消息（head）与最近若干条消息（tail）。
pub async fn summarize_prefix(
    config: &LlmConfigInternal,
    messages: &mut Vec<Value>,
) -> Result<(), AgentError> {
    let head_len = count_head_system_messages(messages);
    if messages.len() <= head_len + KEEP_RECENT_TAIL {
        return Ok(()); // 没有足够的前缀可摘要
    }

    // 选择 tail 起点：取尾部 KEEP_RECENT_TAIL 条，并跳过开头的 tool 消息避免孤儿。
    let mut tail_start = messages.len().saturating_sub(KEEP_RECENT_TAIL);
    if tail_start < head_len {
        tail_start = head_len;
    }
    while tail_start < messages.len()
        && messages[tail_start].get("role").and_then(Value::as_str) == Some("tool")
    {
        tail_start += 1;
    }
    if tail_start <= head_len {
        return Ok(()); // 没有可摘要的中间段
    }

    let middle: Vec<&Value> = messages[head_len..tail_start].iter().collect();
    let mut transcript = String::new();
    for message in &middle {
        transcript.push_str(&render_message_for_summary(message));
        transcript.push('\n');
    }

    let summary_prompt = json!({
        "role": "system",
        "content": "请把以下对话历史压缩为紧凑要点：保留关键决策、已完成的操作、用户意图与未决问题，省略冗余工具细节。用不超过 400 字的中文输出，不要寒暄。"
    });
    let user_prompt = json!({
        "role": "user",
        "content": format!("对话历史：\n{transcript}")
    });

    let summary = chat_completions(config, &[summary_prompt, user_prompt], &[])
        .await
        .map(|message| content_to_text(&message.content))?;

    let summary_message = json!({
        "role": "system",
        "content": format!("## 对话摘要（自动压缩）\n{summary}")
    });

    let mut compacted = Vec::with_capacity(head_len + 1 + (messages.len() - tail_start));
    compacted.extend_from_slice(&messages[..head_len]);
    compacted.push(summary_message);
    compacted.extend_from_slice(&messages[tail_start..]);
    *messages = compacted;
    Ok(())
}

/// 自动上下文压缩主入口：按预算先裁剪工具结果/截图，仍超预算再对早期多轮做 LLM 摘要。
pub async fn compact(
    config: &LlmConfigInternal,
    messages: &mut Vec<Value>,
    tools: &[Value],
    budget: usize,
) -> Result<(), AgentError> {
    if budget == 0 {
        return Ok(());
    }
    if estimate_tokens(messages, tools) <= budget {
        return Ok(());
    }
    trim_tool_results(messages, KEEP_RECENT_TOOL_RESULTS);
    trim_old_images(messages, KEEP_RECENT_IMAGES);
    if estimate_tokens(messages, tools) <= budget {
        return Ok(());
    }
    summarize_prefix(config, messages).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::skills::READ_SKILL_TOOL_NAME;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    fn assistant_with_tool_calls(id: &str, name: &str) -> Value {
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": "{}" }
            }]
        })
    }

    fn tool_result(id: &str, content: &str) -> Value {
        json!({
            "role": "tool",
            "tool_call_id": id,
            "content": content
        })
    }

    #[test]
    fn resolve_budget_defaults_to_256k_when_unconfigured() {
        let mut config = config_with("http://unused/v1".into());
        config.context_window = None;
        config.max_input_tokens = None;
        assert_eq!(
            resolve_budget(&config),
            Some((DEFAULT_CONTEXT_WINDOW as usize * 7) / 10)
        );
    }

    #[test]
    fn estimate_tokens_grows_with_text_and_images_dominate() {
        let small = vec![json!({"role":"user","content":"hi"})];
        let large = vec![json!({"role":"user","content":"x".repeat(3000)})];
        assert!(estimate_tokens(&large, &[]) > estimate_tokens(&small, &[]));

        let image = vec![json!({
            "role":"user","content":[
                {"type":"text","text":"看图"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,".to_string() + &"A".repeat(200000)}}
            ]
        })];
        // 图片估算至少为地板值 1500，远超等长文本
        assert!(estimate_tokens(&image, &[]) >= IMAGE_TOKEN_FLOOR);
        assert!(estimate_tokens(&image, &[]) > estimate_tokens(&vec![json!({
            "role":"user","content":"x".repeat(200)
        })], &[]));
    }

    #[test]
    fn trim_tool_results_keeps_recent_and_protects_read_skill() {
        let read_skill_id = "skill-call";
        let mut messages = vec![
            json!({"role":"system","content":"sys"}),
            assistant_with_tool_calls("a", "get_editor_snapshot"),
            tool_result("a", &"x".repeat(500)),
            assistant_with_tool_calls(read_skill_id, READ_SKILL_TOOL_NAME),
            tool_result(read_skill_id, "技能指令正文"),
            assistant_with_tool_calls("b", "get_editor_snapshot"),
            tool_result("b", &"y".repeat(500)),
            assistant_with_tool_calls("c", "get_editor_snapshot"),
            tool_result("c", &"z".repeat(500)),
            assistant_with_tool_calls("d", "get_editor_snapshot"),
            tool_result("d", &"w".repeat(500)),
        ];

        trim_tool_results(&mut messages, 2);

        // read_skill 结果始终保留
        let skill_msg = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some(read_skill_id))
            .unwrap();
        assert_eq!(skill_msg["content"], "技能指令正文");

        // 最近 2 条可裁剪的 tool 消息（c、d）保留原文
        let d = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("d"))
            .unwrap();
        assert_eq!(d["content"], "w".repeat(500));

        // 更早的可裁剪 tool 消息（a、b）被替换为占位
        let a = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("a"))
            .unwrap();
        assert!(a["content"].as_str().unwrap().contains("[tool result trimmed:"));

        // 结构合法：每条 assistant tool_call 仍对应一条 tool 消息
        let tool_call_ids: Vec<&str> = messages
            .iter()
            .filter_map(|m| {
                m.get("tool_calls")
                    .and_then(Value::as_array)
                    .and_then(|c| c.first())
                    .and_then(|c| c.get("id"))
                    .and_then(Value::as_str)
            })
            .collect();
        for id in tool_call_ids {
            assert!(messages
                .iter()
                .any(|m| m.get("tool_call_id").and_then(Value::as_str) == Some(id)));
        }
    }

    #[test]
    fn trim_old_images_replaces_old_image_parts_only() {
        let mut messages = vec![
            json!({"role":"system","content":"sys"}),
            json!({"role":"user","content":[
                {"type":"text","text":"旧图"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,OLD"}}
            ]}),
            json!({"role":"assistant","content":"好的"}),
            json!({"role":"user","content":[
                {"type":"text","text":"新图"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,NEW"}}
            ]}),
        ];
        trim_old_images(&mut messages, 1);
        let old = &messages[1];
        assert!(old["content"].as_str().unwrap().contains("旧图"));
        assert!(old["content"].as_str().unwrap().contains("[图片已隐藏"));
        let new = &messages[3];
        let parts = new["content"].as_array().unwrap();
        assert!(parts
            .iter()
            .any(|p| p.get("type").and_then(Value::as_str) == Some("image_url")));
    }

    async fn spawn_mock_llm(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let bodies = Arc::new(Mutex::new(vec![body]));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 65536];
                let _ = socket.read(&mut buf).await;
                let reply = {
                    let mut guard = bodies.lock().await;
                    guard.pop().unwrap_or_else(|| {
                        r#"{"choices":[{"message":{"role":"assistant","content":"摘要"}}]}"#.into()
                    })
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(),
                    reply
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{addr}/v1")
    }

    fn config_with(base_url: String) -> LlmConfigInternal {
        LlmConfigInternal {
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        }
    }

    #[tokio::test]
    async fn compact_under_budget_leaves_messages_untouched() {
        let config = config_with("http://unused/v1".into());
        let mut messages = vec![
            json!({"role":"system","content":"sys"}),
            json!({"role":"user","content":"hi"}),
            json!({"role":"assistant","content":"hello"}),
        ];
        let before = messages.clone();
        compact(&config, &mut messages, &[], 1_000_000).await.unwrap();
        assert_eq!(messages, before);
    }

    #[tokio::test]
    async fn compact_trims_tool_results_without_llm_call_when_sufficient() {
        // 没有 mock server：若仍超预算会尝试 LLM 调用并失败。这里让裁剪后落到预算内。
        let config = config_with("http://unused/v1".into());
        let big = "x".repeat(3000);
        let mut messages = vec![json!({"role":"system","content":"sys"})];
        for id in ["a", "b", "c", "d", "e", "f"] {
            messages.push(assistant_with_tool_calls(id, "get_editor_snapshot"));
            messages.push(tool_result(id, &big));
        }
        messages.push(json!({"role":"user","content":"继续"}));

        // 预算设为仅够容纳裁剪后内容：保留最近 4 条 tool 结果，裁剪最旧 2 条为占位。
        let post_trim = estimate_tokens(
            &{
                let mut v = vec![json!({"role":"system","content":"sys"})];
                for id in ["a", "b"] {
                    v.push(assistant_with_tool_calls(id, "get_editor_snapshot"));
                    v.push(tool_result(id, "[tool result trimmed: 3000 chars]"));
                }
                for id in ["c", "d", "e", "f"] {
                    v.push(assistant_with_tool_calls(id, "get_editor_snapshot"));
                    v.push(tool_result(id, &big));
                }
                v.push(json!({"role":"user","content":"继续"}));
                v
            },
            &[],
        );
        let pre_trim = estimate_tokens(&messages, &[]);
        assert!(post_trim < pre_trim, "post_trim must be smaller");
        let budget = post_trim + (pre_trim - post_trim) / 2; // 介于两者之间

        compact(&config, &mut messages, &[], budget).await.unwrap();

        // 最旧 2 条（a、b）被裁剪为占位
        let a = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("a"))
            .unwrap();
        assert!(a["content"].as_str().unwrap().contains("[tool result trimmed:"));
        // 最近 4 条保留原文
        let f = messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("f"))
            .unwrap();
        assert_eq!(f["content"], big);
    }

    #[tokio::test]
    async fn summarize_prefix_replaces_middle_with_summary_message() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"已压缩要点"}}]}"#;
        let base_url = spawn_mock_llm(body.into()).await;
        let config = config_with(base_url);
        let mut messages = vec![
            json!({"role":"system","content":"系统提示"}),
            json!({"role":"system","content":"技能目录"}),
        ];
        for i in 1..=10 {
            messages.push(json!({"role":"user","content":format!("第{i}轮问题")}));
            messages.push(json!({"role":"assistant","content":format!("第{i}轮回答")}));
        }
        let len_before = messages.len();
        summarize_prefix(&config, &mut messages).await.unwrap();
        // head(2) + summary(1) + tail(KEEP_RECENT_TAIL=6) = 9 < 22
        assert!(messages.len() < len_before);
        // head 系统消息保留
        assert_eq!(messages[0]["content"], "系统提示");
        assert_eq!(messages[1]["content"], "技能目录");
        // 摘要消息存在且为 system
        assert!(messages
            .iter()
            .any(|m| m.get("role").and_then(Value::as_str) == Some("system")
                && m.get("content").and_then(Value::as_str).unwrap().contains("已压缩要点")));
        // 最近一轮的问题保留
        assert!(messages
            .iter()
            .any(|m| m.get("content").and_then(Value::as_str) == Some("第10轮问题")));
        // 没有引入孤儿 tool 消息
        assert!(messages
            .iter()
            .all(|m| m.get("role").and_then(Value::as_str) != Some("tool")));
    }
}


