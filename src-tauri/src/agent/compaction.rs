use crate::agent::llm::{chat_completions, content_to_text};
use crate::agent::store::LlmConfigInternal;
use crate::agent::AgentError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

const TEXT_CHARS_PER_TOKEN: usize = 3;
const IMAGE_TOKEN_FLOOR: usize = 1500;
const IMAGE_BASE64_DIVISOR: usize = 100;
const KEEP_RECENT_TOOL_RESULTS: usize = 4;
const KEEP_RECENT_IMAGES: usize = 2;
const KEEP_RECENT_TAIL: usize = 6;
const SUMMARY_OUTPUT_RESERVE: usize = 512;
const SUMMARY_PROMPT_OVERHEAD: usize = 400;
const FINAL_OUTPUT_RESERVE: usize = 1024;
const DETERMINISTIC_MESSAGE_CHARS: usize = 1_200;
const DEFAULT_CONTEXT_WINDOW: u32 = 256_000;

type SummarizeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + 'a>>;

/// 解析输入 token 预算：优先 max_input_tokens，否则按 context_window 的 70% 估算；
/// context_window 未配置时按默认 256k 兜底。
pub fn resolve_budget(config: &LlmConfigInternal) -> Option<usize> {
    if let Some(max_input) = config.max_input_tokens {
        return Some(max_input as usize);
    }
    let window = config.context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);
    Some((window as usize * 7) / 10)
}

fn final_input_budget(configured_budget: usize) -> usize {
    configured_budget.saturating_sub(FINAL_OUTPUT_RESERVE.min(configured_budget / 4))
}

fn summarization_request_budget(configured_budget: usize) -> usize {
    configured_budget
        .saturating_sub(SUMMARY_OUTPUT_RESERVE)
        .max(64)
}

fn summarization_input_budget(configured_budget: usize) -> usize {
    summarization_request_budget(configured_budget)
        .saturating_sub(SUMMARY_PROMPT_OVERHEAD)
        .max(32)
}

/// 估算消息序列 + 工具定义的 token 数（偏保守）。
pub fn estimate_tokens(messages: &[Value], tools: &[Value]) -> usize {
    let mut total = 0usize;
    for message in messages {
        total += estimate_message_tokens(message);
        total += 4;
    }
    if !tools.is_empty() {
        let serialized = serde_json::to_string(tools).unwrap_or_default();
        total += serialized.len() / TEXT_CHARS_PER_TOKEN;
    }
    total
}

fn estimate_message_tokens(message: &Value) -> usize {
    let mut tokens = 0usize;
    match message.get("content") {
        Some(Value::String(text)) => tokens += text_tokens(text),
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
                    tokens += text_tokens(text);
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
                tokens += text_tokens(args);
            }
            tokens += 8;
        }
    }
    tokens
}

fn text_tokens(text: &str) -> usize {
    text.chars().count() / TEXT_CHARS_PER_TOKEN + usize::from(!text.is_empty())
}

fn image_tokens(data_url: &str) -> usize {
    (data_url.len() / IMAGE_BASE64_DIVISOR).max(IMAGE_TOKEN_FLOOR)
}

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

/// 裁剪旧工具结果；read_skill 结果始终保留，且不删除 tool 消息以保持结构合法。
pub fn trim_tool_results(messages: &mut Vec<Value>, keep_recent: usize) {
    let index = build_tool_call_index(messages);
    let read_skill = crate::agent::skills::READ_SKILL_TOOL_NAME;
    let mut trimmable: Vec<usize> = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        if message.get("role").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        let tool_call_id = message
            .get("tool_call_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        if index.get(tool_call_id).map(String::as_str) == Some(read_skill) {
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
        if let Some(obj) = messages[idx].as_object_mut() {
            obj.insert(
                "content".into(),
                Value::String(format!("[tool result trimmed: {original} chars]")),
            );
        }
    }
}

/// 裁剪旧消息中的图片，保留最近 `keep_recent` 条含图消息。
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
        messages[idx]["content"] = Value::String(if joined_text.is_empty() {
            "[图片已隐藏：超出上下文预算，早期截图已压缩]".into()
        } else {
            joined_text
        });
    }
}

fn count_head_system_messages(messages: &[Value]) -> usize {
    messages
        .iter()
        .take_while(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .count()
}

fn role_of(message: &Value) -> &str {
    message.get("role").and_then(Value::as_str).unwrap_or("?")
}

fn has_tool_calls(message: &Value) -> bool {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty())
}

/// assistant(tool_calls) 与对应 tool 结果同组，压缩时不可拆开。
fn atomic_groups(messages: &[Value]) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < messages.len() {
        if role_of(&messages[index]) == "assistant" && has_tool_calls(&messages[index]) {
            let start = index;
            index += 1;
            while index < messages.len() && role_of(&messages[index]) == "tool" {
                index += 1;
            }
            groups.push((start..index).collect());
            continue;
        }
        groups.push(vec![index]);
        index += 1;
    }
    groups
}

fn choose_tail_start(messages: &[Value], head_len: usize) -> usize {
    if messages.len() <= head_len + KEEP_RECENT_TAIL {
        return head_len;
    }
    let groups = atomic_groups(messages);
    let mut retained = 0usize;
    let mut tail_group = groups.len();
    while tail_group > 0 && retained < KEEP_RECENT_TAIL {
        tail_group -= 1;
        if groups[tail_group][0] < head_len {
            tail_group += 1;
            break;
        }
        retained += groups[tail_group].len();
    }
    if tail_group >= groups.len() {
        return head_len;
    }
    groups[tail_group][0].max(head_len)
}

fn render_message_for_summary(message: &Value) -> String {
    let role = role_of(message);
    let content = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| {
                if part.get("type").and_then(Value::as_str) == Some("image_url") {
                    Some("[image]".to_string())
                } else {
                    part.get("text").and_then(Value::as_str).map(str::to_string)
                }
            })
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
    if let Some(tool_call_id) = message.get("tool_call_id").and_then(Value::as_str) {
        line.push_str(&format!("\n  <- tool_call_id={tool_call_id}"));
    }
    line
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!(
            "{head}…[truncated {} chars]",
            value.chars().count().saturating_sub(limit)
        )
    } else {
        head
    }
}

fn deterministic_truncate_transcript(transcript: &str, token_budget: usize) -> String {
    if text_tokens(transcript) <= token_budget {
        return transcript.to_string();
    }
    let char_budget = token_budget.saturating_mul(TEXT_CHARS_PER_TOKEN).max(64);
    let bytes = transcript.len();
    let chars = transcript.chars().count();
    let truncated = truncate_chars(transcript, char_budget);
    format!(
        "[deterministic truncation: {bytes} bytes / {chars} chars → budget {token_budget} tokens]\n{truncated}"
    )
}

fn chunk_middle_transcript(middle: &[Value], summarization_budget: usize) -> Vec<String> {
    if middle.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for group in atomic_groups(middle) {
        let mut group_text = String::new();
        for &idx in &group {
            group_text.push_str(&render_message_for_summary(&middle[idx]));
            group_text.push('\n');
        }
        if text_tokens(&group_text) > summarization_budget {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.push(deterministic_truncate_transcript(
                &group_text,
                summarization_budget,
            ));
            continue;
        }
        let combined = if current.is_empty() {
            group_text.clone()
        } else {
            format!("{current}{group_text}")
        };
        if text_tokens(&combined) > summarization_budget && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current = group_text;
        } else {
            current = combined;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn summary_request_messages(transcript: &str) -> Vec<Value> {
    vec![
        json!({
            "role": "system",
            "content": "请把以下对话历史压缩为紧凑要点：保留关键决策、已完成的操作、用户意图与未决问题，省略冗余工具细节。用不超过 400 字的中文输出，不要寒暄。"
        }),
        json!({
            "role": "user",
            "content": format!("对话历史：\n{transcript}")
        }),
    ]
}

fn fit_transcript_for_summary_request(transcript: &str, request_budget: usize) -> String {
    if estimate_tokens(&summary_request_messages(transcript), &[]) <= request_budget {
        return transcript.to_string();
    }
    let overhead = estimate_tokens(&summary_request_messages(""), &[]);
    let token_room = request_budget.saturating_sub(overhead).max(16);
    deterministic_truncate_transcript(transcript, token_room)
}

fn chunk_plain_text(text: &str, budget: usize) -> Vec<String> {
    if text_tokens(text) <= budget {
        return vec![text.to_string()];
    }
    let char_budget = budget.saturating_mul(TEXT_CHARS_PER_TOKEN).max(64);
    text.chars()
        .collect::<Vec<_>>()
        .chunks(char_budget)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn ensure_within_budget(messages: &[Value], tools: &[Value], budget: usize) -> Result<(), AgentError> {
    let used = estimate_tokens(messages, tools);
    if used > budget {
        return Err(AgentError::new(
            "context_uncompressible",
            format!("压缩请求超出预算：estimated={used}, budget={budget}"),
        ));
    }
    Ok(())
}

fn context_uncompressible(detail: impl Into<String>) -> AgentError {
    AgentError::new(
        "context_uncompressible",
        format!(
            "上下文无法压缩到预算内：{}。请缩小工具结果、系统提示或工具定义后重试。",
            detail.into()
        ),
    )
}

async fn summarize_transcript_chunks<F>(
    chunks: Vec<String>,
    transcript_budget: usize,
    request_budget: usize,
    mut summarize: F,
) -> Result<String, AgentError>
where
    F: for<'a> FnMut(&'a [Value]) -> SummarizeFuture<'a>,
{
    if chunks.is_empty() {
        return Ok(String::new());
    }

    let mut level = chunks;
    loop {
        let mut next = Vec::with_capacity(level.len());
        for chunk in &level {
            let payload = fit_transcript_for_summary_request(chunk, request_budget);
            let request = summary_request_messages(&payload);
            if estimate_tokens(&request, &[]) > request_budget {
                next.push(payload);
                continue;
            }
            ensure_within_budget(&request, &[], request_budget)?;
            next.push(summarize(&request).await?);
        }

        if next.len() == 1 {
            return Ok(next.pop().unwrap_or_default());
        }
        level = chunk_plain_text(&next.join("\n"), transcript_budget);
    }
}

fn llm_summarizer<'a>(
    config: &'a LlmConfigInternal,
) -> impl for<'b> FnMut(&'b [Value]) -> SummarizeFuture<'b> + 'a {
    move |request: &[Value]| {
        let config = config.clone();
        let request = request.to_vec();
        Box::pin(async move {
            let message = chat_completions(&config, &request, &[]).await?;
            Ok(content_to_text(&message.content))
        })
    }
}

async fn summarize_prefix_with<F>(
    messages: &mut Vec<Value>,
    configured_budget: usize,
    summarize: F,
) -> Result<(), AgentError>
where
    F: for<'a> FnMut(&'a [Value]) -> SummarizeFuture<'a>,
{
    let head_len = count_head_system_messages(messages);
    let tail_start = choose_tail_start(messages, head_len);
    if tail_start <= head_len {
        return Ok(());
    }

    let middle = messages[head_len..tail_start].to_vec();
    let transcript_budget = summarization_input_budget(configured_budget);
    let request_budget = summarization_request_budget(configured_budget);
    let chunks = chunk_middle_transcript(&middle, transcript_budget);
    let summary =
        summarize_transcript_chunks(chunks, transcript_budget, request_budget, summarize).await?;

    let mut compacted = Vec::with_capacity(head_len + 1 + (messages.len() - tail_start));
    compacted.extend_from_slice(&messages[..head_len]);
    compacted.push(json!({
        "role": "system",
        "content": format!("## 对话摘要（自动压缩）\n{summary}")
    }));
    compacted.extend_from_slice(&messages[tail_start..]);
    *messages = compacted;
    Ok(())
}

fn deterministic_force_fit(messages: &mut Vec<Value>, tools: &[Value], budget: usize) {
    let head_len = count_head_system_messages(messages);
    for index in head_len..messages.len() {
        if estimate_tokens(messages, tools) <= budget {
            return;
        }
        let role = role_of(&messages[index]);
        let is_summary = role == "system"
            && messages[index]
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains("对话摘要（自动压缩）"));
        if is_summary {
            if let Some(text) = messages[index].get("content").and_then(Value::as_str) {
                messages[index]["content"] =
                    Value::String(truncate_chars(text, DETERMINISTIC_MESSAGE_CHARS));
            }
            continue;
        }
        if !matches!(role, "tool" | "assistant" | "user") {
            continue;
        }
        match messages[index].get("content").cloned() {
            Some(Value::String(text)) if text.chars().count() > DETERMINISTIC_MESSAGE_CHARS => {
                let bytes = text.len();
                let chars = text.chars().count();
                let truncated = truncate_chars(&text, DETERMINISTIC_MESSAGE_CHARS);
                messages[index]["content"] = Value::String(format!(
                    "[lossy truncate: {bytes} bytes / {chars} chars]\n{truncated}"
                ));
            }
            Some(Value::Array(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("");
                messages[index]["content"] = Value::String(truncate_chars(
                    &format!("[multimodal reduced] {text}"),
                    DETERMINISTIC_MESSAGE_CHARS,
                ));
            }
            _ => {}
        }
    }
}

fn enforce_final_budget(
    messages: &mut Vec<Value>,
    tools: &[Value],
    budget: usize,
) -> Result<(), AgentError> {
    if estimate_tokens(messages, tools) <= budget {
        return Ok(());
    }
    deterministic_force_fit(messages, tools, budget);
    let used = estimate_tokens(messages, tools);
    if used <= budget {
        return Ok(());
    }
    let tools_cost = estimate_tokens(&[], tools);
    let head_len = count_head_system_messages(messages);
    let head_cost = estimate_tokens(&messages[..head_len], &[]);
    Err(context_uncompressible(format!(
        "fixed components exceed limit (tools={tools_cost}, head={head_cost}, total={used}, budget={budget})"
    )))
}

/// 自动上下文压缩：裁剪工具结果/截图 → 有界摘要 → 强制最终请求落在输入预算内。
pub async fn compact(
    config: &LlmConfigInternal,
    messages: &mut Vec<Value>,
    tools: &[Value],
    budget: usize,
) -> Result<(), AgentError> {
    compact_with(messages, tools, budget, llm_summarizer(config)).await
}

async fn compact_with<F>(
    messages: &mut Vec<Value>,
    tools: &[Value],
    budget: usize,
    summarize: F,
) -> Result<(), AgentError>
where
    F: for<'a> FnMut(&'a [Value]) -> SummarizeFuture<'a>,
{
    if budget == 0 {
        return Ok(());
    }
    let input_budget = final_input_budget(budget).max(1);
    if estimate_tokens(messages, tools) <= input_budget {
        return Ok(());
    }
    trim_tool_results(messages, KEEP_RECENT_TOOL_RESULTS);
    trim_old_images(messages, KEEP_RECENT_IMAGES);
    if estimate_tokens(messages, tools) <= input_budget {
        return Ok(());
    }
    summarize_prefix_with(messages, budget, summarize).await?;
    enforce_final_budget(messages, tools, input_budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::skills::READ_SKILL_TOOL_NAME;
    use serde_json::json;
    use std::sync::Arc;
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

    fn recording_summarizer(
        recorded: Arc<Mutex<Vec<usize>>>,
        request_budget: usize,
        reply: &'static str,
    ) -> impl for<'a> FnMut(&'a [Value]) -> SummarizeFuture<'a> {
        move |request: &[Value]| {
            let recorded = recorded.clone();
            let request = request.to_vec();
            Box::pin(async move {
                let used = estimate_tokens(&request, &[]);
                recorded.lock().await.push(used);
                assert!(used <= request_budget, "{used} > {request_budget}");
                Ok(reply.into())
            })
        }
    }

    #[test]
    fn resolve_budget_defaults_to_256k_when_unconfigured() {
        let config = LlmConfigInternal {
            base_url: Some("http://unused/v1".into()),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        assert_eq!(
            resolve_budget(&config),
            Some((DEFAULT_CONTEXT_WINDOW as usize * 7) / 10)
        );
    }

    #[test]
    fn estimate_tokens_grows_with_text_images_and_cjk_chars() {
        let small = vec![json!({"role":"user","content":"hi"})];
        let large = vec![json!({"role":"user","content":"x".repeat(3000)})];
        assert!(estimate_tokens(&large, &[]) > estimate_tokens(&small, &[]));

        let image = vec![json!({
            "role":"user","content":[
                {"type":"text","text":"看图"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,".to_string() + &"A".repeat(200000)}}
            ]
        })];
        assert!(estimate_tokens(&image, &[]) >= IMAGE_TOKEN_FLOOR);

        let ascii = vec![json!({"role":"user","content":"abc".repeat(100)})];
        let cjk = vec![json!({"role":"user","content":"参数".repeat(100)})];
        let ascii_tokens = estimate_tokens(&ascii, &[]);
        let cjk_tokens = estimate_tokens(&cjk, &[]);
        assert!((ascii_tokens as i64 - cjk_tokens as i64).abs() < 40);
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

        assert_eq!(
            messages
                .iter()
                .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some(read_skill_id))
                .unwrap()["content"],
            "技能指令正文"
        );
        assert_eq!(
            messages
                .iter()
                .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("d"))
                .unwrap()["content"],
            "w".repeat(500)
        );
        assert!(messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("a"))
            .unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("[tool result trimmed:"));
        for message in &messages {
            if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let id = call.get("id").and_then(Value::as_str).unwrap();
                    assert!(messages
                        .iter()
                        .any(|m| m.get("tool_call_id").and_then(Value::as_str) == Some(id)));
                }
            }
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
        let old = messages[1]["content"].as_str().unwrap();
        assert!(old.contains("旧图") && old.contains("[图片已隐藏"));
        assert!(messages[3]["content"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.get("type").and_then(Value::as_str) == Some("image_url")));
    }

    #[test]
    fn atomic_groups_and_chunks_keep_tool_pairs() {
        let messages = vec![
            json!({"role":"system","content":"sys"}),
            assistant_with_tool_calls("a", "read_skill"),
            tool_result("a", "skill"),
            json!({"role":"user","content":"继续"}),
        ];
        assert_eq!(atomic_groups(&messages), vec![vec![0], vec![1, 2], vec![3]]);

        let budget = 200;
        let mut middle = Vec::new();
        for i in 0..8 {
            let id = format!("call-{i}");
            middle.push(assistant_with_tool_calls(&id, "get_editor_snapshot"));
            middle.push(tool_result(&id, &format!("结果{i}：{}", "参数".repeat(80))));
        }
        let chunks = chunk_middle_transcript(&middle, budget);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(text_tokens(chunk) <= budget + 8);
            assert_eq!(chunk.matches("-> call").count(), chunk.matches("tool_call_id=").count());
        }
    }

    #[tokio::test]
    async fn compact_under_budget_leaves_messages_untouched() {
        let config = LlmConfigInternal {
            base_url: Some("http://unused/v1".into()),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        let mut messages = vec![
            json!({"role":"system","content":"sys"}),
            json!({"role":"user","content":"hi"}),
            json!({"role":"assistant","content":"hello"}),
        ];
        let before = messages.clone();
        compact(&config, &mut messages, &[], 1_000_000)
            .await
            .unwrap();
        assert_eq!(messages, before);
    }

    #[tokio::test]
    async fn compact_trims_tool_results_without_llm_when_sufficient() {
        let config = LlmConfigInternal {
            base_url: Some("http://unused/v1".into()),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        let big = "x".repeat(3000);
        let mut messages = vec![json!({"role":"system","content":"sys"})];
        for id in ["a", "b", "c", "d", "e", "f"] {
            messages.push(assistant_with_tool_calls(id, "get_editor_snapshot"));
            messages.push(tool_result(id, &big));
        }
        messages.push(json!({"role":"user","content":"继续"}));

        let mut trimmed = messages.clone();
        trim_tool_results(&mut trimmed, KEEP_RECENT_TOOL_RESULTS);
        let pre = estimate_tokens(&messages, &[]);
        let post = estimate_tokens(&trimmed, &[]);
        let budget = post + (pre - post) / 2 + FINAL_OUTPUT_RESERVE;

        compact(&config, &mut messages, &[], budget).await.unwrap();
        assert!(messages
            .iter()
            .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("a"))
            .unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("[tool result trimmed:"));
        assert_eq!(
            messages
                .iter()
                .find(|m| m.get("tool_call_id").and_then(Value::as_str) == Some("f"))
                .unwrap()["content"],
            big
        );
        assert!(estimate_tokens(&messages, &[]) <= final_input_budget(budget));
    }

    #[tokio::test]
    async fn compact_keeps_every_request_and_final_payload_in_budget() {
        let budget = 4_000;
        let summary_budget = summarization_request_budget(budget);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let large_tools = vec![json!({
            "type":"function",
            "function":{
                "name":"huge_schema_tool",
                "description":"x".repeat(800),
                "parameters":{"type":"object","properties":{
                    "payload":{"type":"string","description":"参数说明".repeat(200)}
                }}
            }
        })];

        let mut messages = vec![
            json!({"role":"system","content":"系统提示：保持建模约束。"}),
            json!({"role":"system","content":"技能目录：object-editing"}),
        ];
        for i in 0..12 {
            let id = format!("skill-{i}");
            messages.push(assistant_with_tool_calls(&id, READ_SKILL_TOOL_NAME));
            messages.push(tool_result(
                &id,
                &format!("技能正文{i}：{}", "必须保留的中文说明".repeat(120)),
            ));
            messages.push(json!({
                "role":"user",
                "content":[
                    {"type":"text","text":format!("请看图并继续第{i}步")},
                    {"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}", "B".repeat(20_000))}}
                ]
            }));
            messages.push(json!({"role":"assistant","content":format!("已处理第{i}步")}));
        }
        assert!(estimate_tokens(&messages, &large_tools) > final_input_budget(budget));

        compact_with(
            &mut messages,
            &large_tools,
            budget,
            recording_summarizer(recorded.clone(), summary_budget, "分层摘要"),
        )
        .await
        .unwrap();

        assert!(estimate_tokens(&messages, &large_tools) <= final_input_budget(budget));
        for used in recorded.lock().await.iter() {
            assert!(*used <= summary_budget);
        }
        for group in atomic_groups(&messages) {
            if group.len() > 1 {
                assert_eq!(role_of(&messages[group[0]]), "assistant");
                assert!(group[1..]
                    .iter()
                    .all(|&idx| role_of(&messages[idx]) == "tool"));
            }
        }
        assert!(messages.iter().any(|m| {
            m.get("role").and_then(Value::as_str) == Some("system")
                && m.get("content")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("分层摘要") || text.contains("对话摘要"))
        }) || estimate_tokens(&messages, &large_tools) <= final_input_budget(budget));
    }

    #[tokio::test]
    async fn oversized_tool_or_fixed_head_stays_bounded_or_errors() {
        let budget = 800;
        let summary_budget = summarization_request_budget(budget);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let mut messages = vec![json!({"role":"system","content":"sys"})];
        for i in 0..10 {
            messages.push(json!({"role":"user","content":format!("q{i}")}));
            messages.push(json!({"role":"assistant","content":format!("a{i}")}));
        }
        messages.push(assistant_with_tool_calls("huge", "get_editor_snapshot"));
        messages.push(tool_result("huge", &"超大".repeat(50_000)));
        messages.push(json!({"role":"user","content":"继续"}));

        let result = compact_with(
            &mut messages,
            &[],
            budget,
            recording_summarizer(recorded.clone(), summary_budget, "摘要"),
        )
        .await;
        match result {
            Ok(()) => assert!(estimate_tokens(&messages, &[]) <= final_input_budget(budget)),
            Err(error) => assert_eq!(error.code, "context_uncompressible"),
        }
        for used in recorded.lock().await.iter() {
            assert!(*used <= summary_budget);
        }

        let mut fixed = vec![
            json!({"role":"system","content":"S".repeat(20_000)}),
            json!({"role":"user","content":"x".repeat(5_000)}),
            json!({"role":"assistant","content":"y".repeat(5_000)}),
        ];
        let tools = vec![json!({
            "type":"function",
            "function":{"name":"t","description":"D".repeat(5_000),"parameters":{"type":"object"}}
        })];
        let error = compact_with(&mut fixed, &tools, 2_000, recording_summarizer(
            Arc::new(Mutex::new(Vec::new())),
            summarization_request_budget(2_000),
            "unused",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, "context_uncompressible");
    }

    #[tokio::test]
    async fn fake_summarizer_chunked_requests_stay_under_limit() {
        let budget = 3_000;
        let summary_budget = summarization_request_budget(budget);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let mut messages = vec![json!({"role":"system","content":"sys"})];
        for i in 0..30 {
            messages.push(json!({"role":"user","content":format!("中文问题{i}{}", "内容".repeat(80))}));
            messages.push(json!({"role":"assistant","content":format!("回答{i}{}", "要点".repeat(80))}));
        }
        assert!(estimate_tokens(&messages, &[]) > final_input_budget(budget));

        compact_with(
            &mut messages,
            &[],
            budget,
            recording_summarizer(recorded.clone(), summary_budget, "要点摘要"),
        )
        .await
        .unwrap();

        for used in recorded.lock().await.iter() {
            assert!(*used <= summary_budget);
        }
        assert!(estimate_tokens(&messages, &[]) <= final_input_budget(budget));
    }
}
