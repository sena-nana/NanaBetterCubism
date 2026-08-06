use crate::agent::store::{LlmApiMode, LlmConfigInternal};
use crate::agent::AgentError;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

const LLM_400_MAX_RETRIES: u32 = 5;

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessagePayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessagePayload {
    #[allow(dead_code)]
    pub role: Option<String>,
    pub content: Option<Value>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallPayload>>,
    #[serde(default)]
    pub response_output: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallPayload {
    pub id: String,
    pub r#type: Option<String>,
    pub function: ToolFunctionPayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolFunctionPayload {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStreamDelta {
    Text(String),
    ToolCall { name: String, arguments: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    Auto,
    Required,
}

#[derive(Debug, Default)]
struct StreamingToolCall {
    id: String,
    r#type: Option<String>,
    name: String,
    arguments: String,
}

fn resolve_endpoint(config: &LlmConfigInternal) -> Result<(String, String, String), AgentError> {
    let base = config
        .base_url
        .as_ref()
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 Base URL。"))?;
    let api_key = config
        .api_key
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 API Key。"))?;
    let model = config
        .model
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 Model。"))?;
    Ok((base, api_key, model))
}

fn request_body(
    base_url: &str,
    model: &str,
    messages: &[Value],
    tools: &[Value],
    tool_choice: ToolChoiceMode,
    stream: bool,
) -> Result<Value, AgentError> {
    let messages = messages
        .iter()
        .map(|message| {
            let mut message = message.clone();
            if let Some(object) = message.as_object_mut() {
                object.remove("__responses_output");
            }
            message
        })
        .collect::<Vec<_>>();
    let deepseek_v4 = is_official_deepseek_v4(base_url, model);
    let deepseek_thinking = deepseek_v4
        && tool_choice != ToolChoiceMode::Required
        && !has_non_thinking_tool_history(&messages);
    let messages = if deepseek_thinking {
        normalize_deepseek_history(&messages)?
    } else {
        messages
    };
    let body = if tools.is_empty() {
        json!({ "model": model, "messages": messages, "stream": stream })
    } else {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "tools": tools,
            "parallel_tool_calls": false,
            "stream": stream,
        });
        if deepseek_v4 {
            body["thinking"] = json!({
                "type": if deepseek_thinking {
                    "enabled"
                } else {
                    "disabled"
                }
            });
            if !deepseek_thinking {
                body["tool_choice"] = json!(tool_choice);
            }
        } else {
            body["tool_choice"] = json!(tool_choice);
        }
        body
    };
    Ok(body)
}

fn is_official_deepseek_v4(base_url: &str, model: &str) -> bool {
    matches!(model, "deepseek-v4-pro" | "deepseek-v4-flash")
        && reqwest::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| host.eq_ignore_ascii_case("api.deepseek.com"))
}

fn has_non_thinking_tool_history(messages: &[Value]) -> bool {
    messages.iter().any(|message| {
        let is_tool_call = message.get("role").and_then(Value::as_str) == Some("assistant")
            && message
                .get("tool_calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| !calls.is_empty());
        is_tool_call
            && message
                .get("reasoning_content")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
    })
}

fn normalize_deepseek_history(messages: &[Value]) -> Result<Vec<Value>, AgentError> {
    let mut normalized = messages.to_vec();
    for message in &mut normalized {
        let is_tool_call = message.get("role").and_then(Value::as_str) == Some("assistant")
            && message
                .get("tool_calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| !calls.is_empty());
        if !is_tool_call {
            continue;
        }
        if message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(AgentError::new(
                "llm_reasoning_history_incomplete",
                "DeepSeek 思考模式的工具调用历史缺少 reasoning_content，已停止发送无效请求。",
            ));
        }
        if message.get("content").is_none_or(Value::is_null) {
            message["content"] = Value::String(String::new());
        }
    }
    Ok(normalized)
}

fn first_message(parsed: ChatCompletionResponse) -> Result<ChatMessagePayload, AgentError> {
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message)
        .ok_or_else(|| AgentError::new("llm_empty", "模型未返回内容。"))
}

/// 判定模型错误体是否表示「不支持图片输入 / image_url」。
/// 覆盖 OpenAI 兼容、Anthropic 代理、Gemini 代理等常见文案，大小写不敏感。
pub fn detect_image_unsupported(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    let contains = |needle: &str| lower.contains(needle);

    let has_image_url = contains("image_url");
    let has_image_input = contains("image input");
    let has_vision = contains("vision");
    let has_multimodal = contains("multimodal");

    let not_supported = contains("not supported")
        || contains("unsupported")
        || contains("does not support")
        || contains("not available")
        || contains("invalid")
        || contains("not exist");

    (has_image_url || has_image_input) && not_supported
        || (has_vision && (contains("not supported") || contains("not available")))
        || (has_multimodal && (contains("not supported") || contains("unsupported")))
}

fn classify_request_failure(status: reqwest::StatusCode, text: String) -> AgentError {
    if detect_image_unsupported(&text) {
        AgentError::new(
            "llm_image_unsupported",
            format!("当前模型不支持图片输入（{status}）。"),
        )
    } else {
        AgentError::new("llm_request_failed", format!("模型请求失败（{status}）。"))
    }
}

async fn post_json_with_400_retry(
    url: &str,
    api_key: &str,
    body: &Value,
) -> Result<reqwest::Response, AgentError> {
    let client = reqwest::Client::new();
    let mut attempt = 0u32;
    loop {
        let response = client
            .post(url)
            .bearer_auth(api_key)
            .json(body)
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.as_u16() == 400
            && !detect_image_unsupported(&text)
            && attempt < LLM_400_MAX_RETRIES
        {
            attempt += 1;
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        return Err(classify_request_failure(status, text));
    }
}

pub async fn complete(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
) -> Result<ChatMessagePayload, AgentError> {
    match config.api_mode {
        LlmApiMode::ChatCompletions => chat_completions(config, messages, tools).await,
        LlmApiMode::Responses => responses(config, messages, tools).await,
    }
}

pub async fn chat_completions(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
) -> Result<ChatMessagePayload, AgentError> {
    let (base, api_key, model) = resolve_endpoint(config)?;
    let response = post_json_with_400_retry(
        &format!("{base}/chat/completions"),
        &api_key,
        &request_body(&base, &model, messages, tools, ToolChoiceMode::Auto, false)?,
    )
    .await?;
    first_message(response.json().await?)
}

pub async fn chat_completions_stream<F>(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
    tool_choice: ToolChoiceMode,
    mut on_delta: F,
) -> Result<ChatMessagePayload, AgentError>
where
    F: FnMut(ChatStreamDelta),
{
    let (base, api_key, model) = resolve_endpoint(config)?;
    let response = post_json_with_400_retry(
        &format!("{base}/chat/completions"),
        &api_key,
        &request_body(&base, &model, messages, tools, tool_choice, true)?,
    )
    .await?;

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if !content_type.contains("text/event-stream") {
        let message = first_message(response.json().await?)?;
        let text = content_to_text(&message.content);
        if !text.is_empty() {
            on_delta(ChatStreamDelta::Text(text));
        }
        return Ok(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut tool_calls: BTreeMap<u64, StreamingToolCall> = BTreeMap::new();
    let mut finished = false;

    while !finished {
        let Some(chunk) = stream.next().await else {
            break;
        };
        let chunk = chunk?;
        buffer.extend_from_slice(&chunk);
        while let Some(line) = take_sse_line(&mut buffer)? {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                finished = true;
                break;
            }
            let Ok(payload) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            let Some(choice) = payload
                .get("choices")
                .and_then(|choices| choices.as_array())
                .and_then(|choices| choices.first())
            else {
                continue;
            };
            let Some(delta) = choice.get("delta") else {
                continue;
            };
            if let Some(piece) = delta.get("content").and_then(|value| value.as_str()) {
                if !piece.is_empty() {
                    content.push_str(piece);
                    on_delta(ChatStreamDelta::Text(piece.to_string()));
                }
            }
            if let Some(piece) = delta
                .get("reasoning_content")
                .and_then(|value| value.as_str())
            {
                reasoning_content.push_str(piece);
            }
            if let Some(calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
                for call in calls {
                    let index = call
                        .get("index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    let entry = tool_calls.entry(index).or_default();
                    let mut changed = false;
                    if let Some(id) = call.get("id").and_then(|value| value.as_str()) {
                        if !id.is_empty() {
                            entry.id = id.to_string();
                        }
                    }
                    if let Some(kind) = call.get("type").and_then(|value| value.as_str()) {
                        entry.r#type = Some(kind.to_string());
                    }
                    if let Some(function) = call.get("function") {
                        if let Some(name) = function.get("name").and_then(|value| value.as_str()) {
                            if !name.is_empty() {
                                entry.name.push_str(name);
                                changed = true;
                            }
                        }
                        if let Some(arguments) =
                            function.get("arguments").and_then(|value| value.as_str())
                        {
                            if !arguments.is_empty() {
                                entry.arguments.push_str(arguments);
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        on_delta(ChatStreamDelta::ToolCall {
                            name: entry.name.clone(),
                            arguments: entry.arguments.clone(),
                        });
                    }
                }
            }
        }
    }

    let tool_calls = if tool_calls.is_empty() {
        None
    } else {
        Some(
            tool_calls
                .into_values()
                .map(|call| ToolCallPayload {
                    id: if call.id.is_empty() {
                        crate::agent::new_id()
                    } else {
                        call.id
                    },
                    r#type: call.r#type.or_else(|| Some("function".into())),
                    function: ToolFunctionPayload {
                        name: call.name,
                        arguments: call.arguments,
                    },
                })
                .collect(),
        )
    };

    Ok(ChatMessagePayload {
        role: Some("assistant".into()),
        content: if content.is_empty() {
            None
        } else {
            Some(Value::String(content))
        },
        reasoning_content: if reasoning_content.is_empty() {
            None
        } else {
            Some(reasoning_content)
        },
        tool_calls,
        response_output: None,
    })
}

pub async fn complete_stream<F>(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
    tool_choice: ToolChoiceMode,
    on_delta: F,
) -> Result<ChatMessagePayload, AgentError>
where
    F: FnMut(ChatStreamDelta),
{
    match config.api_mode {
        LlmApiMode::ChatCompletions => {
            chat_completions_stream(config, messages, tools, tool_choice, on_delta).await
        }
        LlmApiMode::Responses => {
            responses_stream(config, messages, tools, tool_choice, on_delta).await
        }
    }
}

fn responses_request_body(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    tool_choice: ToolChoiceMode,
    stream: bool,
) -> Result<Value, AgentError> {
    let mut body = json!({
        "model": model,
        "input": responses_input(messages)?,
        "store": false,
        "stream": stream,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(responses_tools(tools)?);
        body["tool_choice"] = serde_json::to_value(tool_choice)?;
        body["parallel_tool_calls"] = Value::Bool(false);
    }
    Ok(body)
}

fn responses_input(messages: &[Value]) -> Result<Vec<Value>, AgentError> {
    let mut input = Vec::new();
    for message in messages {
        if let Some(items) = message.get("__responses_output").and_then(Value::as_array) {
            input.extend(items.iter().cloned());
            continue;
        }

        let role = message
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::new("llm_request_invalid", "模型消息缺少角色。"))?;
        if role == "tool" {
            let call_id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AgentError::new("llm_request_invalid", "工具结果缺少调用标识。"))?;
            input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": response_output_string(message.get("content")),
            }));
            continue;
        }

        if role == "assistant" {
            let text = message
                .get("content")
                .map(|content| content_to_text(&Some(content.clone())))
                .unwrap_or_default();
            if !text.is_empty() {
                input.push(json!({ "role": "assistant", "content": text }));
            }
            if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let function = call.get("function").ok_or_else(|| {
                        AgentError::new("llm_request_invalid", "工具调用缺少函数定义。")
                    })?;
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.get("id").and_then(Value::as_str).ok_or_else(|| AgentError::new("llm_request_invalid", "工具调用缺少调用标识。"))?,
                        "name": function.get("name").and_then(Value::as_str).ok_or_else(|| AgentError::new("llm_request_invalid", "工具调用缺少函数名称。"))?,
                        "arguments": function.get("arguments").and_then(Value::as_str).unwrap_or("{}"),
                    }));
                }
            }
            continue;
        }

        input.push(json!({
            "role": role,
            "content": responses_message_content(message.get("content"))?,
        }));
    }
    Ok(input)
}

fn responses_message_content(content: Option<&Value>) -> Result<Value, AgentError> {
    let Some(content) = content else {
        return Ok(Value::String(String::new()));
    };
    let Value::Array(parts) = content else {
        return Ok(content.clone());
    };
    let mut converted = Vec::with_capacity(parts.len());
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("text" | "input_text") => converted.push(json!({
                "type": "input_text",
                "text": part.get("text").and_then(Value::as_str).unwrap_or_default(),
            })),
            Some("image_url") => {
                let url = part
                    .get("image_url")
                    .and_then(|image| image.get("url"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AgentError::new("llm_request_invalid", "图片消息缺少图片地址。")
                    })?;
                converted.push(json!({ "type": "input_image", "image_url": url }));
            }
            Some("input_image") => converted.push(part.clone()),
            _ => {
                return Err(AgentError::new(
                    "llm_request_invalid",
                    "模型消息包含不支持的内容类型。",
                ));
            }
        }
    }
    Ok(Value::Array(converted))
}

fn response_output_string(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(value)) => value.clone(),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn responses_tools(tools: &[Value]) -> Result<Vec<Value>, AgentError> {
    tools
        .iter()
        .map(|tool| {
            let function = tool.get("function").ok_or_else(|| {
                AgentError::new("llm_request_invalid", "工具定义缺少函数内容。")
            })?;
            Ok(json!({
                "type": "function",
                "name": function.get("name").and_then(Value::as_str).ok_or_else(|| AgentError::new("llm_request_invalid", "工具定义缺少名称。"))?,
                "description": function.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
                "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                "strict": false,
            }))
    })
        .collect()
}

async fn responses(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
) -> Result<ChatMessagePayload, AgentError> {
    let (base, api_key, model) = resolve_endpoint(config)?;
    let response = post_json_with_400_retry(
        &format!("{base}/responses"),
        &api_key,
        &responses_request_body(&model, messages, tools, ToolChoiceMode::Auto, false)?,
    )
    .await?;
    response_to_message(response.json().await?)
}

fn response_to_message(response: Value) -> Result<ChatMessagePayload, AgentError> {
    if matches!(
        response.get("status").and_then(Value::as_str),
        Some("failed" | "incomplete")
    ) {
        return Err(AgentError::new(
            "llm_response_incomplete",
            "模型未完成本次响应。",
        ));
    }
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| AgentError::new("llm_response_invalid", "模型返回缺少输出。"))?;
    response_output_to_message(output)
}

fn response_output_to_message(output: Vec<Value>) -> Result<ChatMessagePayload, AgentError> {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    for item in &output {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for part in parts {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                content.push_str(text);
                            }
                        }
                    }
                }
            }
            Some("function_call") => {
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AgentError::new("llm_response_invalid", "函数调用缺少调用标识。")
                    })?;
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| AgentError::new("llm_response_invalid", "函数调用缺少名称。"))?;
                tool_calls.push(ToolCallPayload {
                    id: call_id.to_string(),
                    r#type: Some("function".into()),
                    function: ToolFunctionPayload {
                        name: name.to_string(),
                        arguments: item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string(),
                    },
                });
            }
            _ => {}
        }
    }
    if content.is_empty() && tool_calls.is_empty() {
        return Err(AgentError::new("llm_empty", "模型未返回内容。"));
    }
    Ok(ChatMessagePayload {
        role: Some("assistant".into()),
        content: (!content.is_empty()).then_some(Value::String(content)),
        reasoning_content: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        response_output: Some(output),
    })
}

async fn responses_stream<F>(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
    tool_choice: ToolChoiceMode,
    mut on_delta: F,
) -> Result<ChatMessagePayload, AgentError>
where
    F: FnMut(ChatStreamDelta),
{
    let (base, api_key, model) = resolve_endpoint(config)?;
    let response = post_json_with_400_retry(
        &format!("{base}/responses"),
        &api_key,
        &responses_request_body(&model, messages, tools, tool_choice, true)?,
    )
    .await?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("text/event-stream") {
        let message = response_to_message(response.json().await?)?;
        let text = content_to_text(&message.content);
        if !text.is_empty() {
            on_delta(ChatStreamDelta::Text(text));
        }
        return Ok(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut calls: BTreeMap<u64, StreamingToolCall> = BTreeMap::new();
    let mut completed = None;
    while completed.is_none() {
        let Some(chunk) = stream.next().await else {
            break;
        };
        buffer.extend_from_slice(&chunk?);
        while let Some(line) = take_sse_line(&mut buffer)? {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let payload = serde_json::from_str::<Value>(data)
                .map_err(|_| AgentError::new("llm_stream_invalid", "模型流返回了无效事件。"))?;
            let kind = payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match kind {
                "response.output_text.delta" => {
                    if let Some(piece) = payload.get("delta").and_then(Value::as_str) {
                        if !piece.is_empty() {
                            on_delta(ChatStreamDelta::Text(piece.to_string()));
                        }
                    }
                }
                "response.output_item.added" => {
                    let index = payload
                        .get("output_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    if let Some(item) = payload.get("item") {
                        if item.get("type").and_then(Value::as_str) == Some("function_call") {
                            let call = calls.entry(index).or_default();
                            call.id = item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            call.name = item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            call.arguments = item
                                .get("arguments")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                        }
                    }
                }
                "response.function_call_arguments.delta" => {
                    let index = payload
                        .get("output_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let call = calls.entry(index).or_default();
                    if let Some(piece) = payload.get("delta").and_then(Value::as_str) {
                        call.arguments.push_str(piece);
                    }
                    if !call.name.is_empty() {
                        on_delta(ChatStreamDelta::ToolCall {
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        });
                    }
                }
                "response.function_call_arguments.done" => {
                    let index = payload
                        .get("output_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let call = calls.entry(index).or_default();
                    if let Some(arguments) = payload.get("arguments").and_then(Value::as_str) {
                        call.arguments = arguments.to_string();
                    }
                    if !call.name.is_empty() {
                        on_delta(ChatStreamDelta::ToolCall {
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        });
                    }
                }
                "response.completed" => {
                    let response = payload.get("response").ok_or_else(|| {
                        AgentError::new("llm_response_invalid", "模型完成事件缺少响应内容。")
                    })?;
                    if response.get("status").and_then(Value::as_str) != Some("completed") {
                        return Err(AgentError::new(
                            "llm_response_incomplete",
                            "模型未完成本次响应。",
                        ));
                    }
                    let output = response
                        .get("output")
                        .and_then(Value::as_array)
                        .cloned()
                        .ok_or_else(|| {
                            AgentError::new("llm_response_invalid", "模型完成事件缺少输出。")
                        })?;
                    completed = Some(response_output_to_message(output)?);
                }
                "response.failed" | "response.incomplete" | "error" => {
                    return Err(AgentError::new(
                        "llm_response_incomplete",
                        "模型未完成本次响应。",
                    ));
                }
                _ => {}
            }
        }
    }
    completed.ok_or_else(|| AgentError::new("llm_stream_incomplete", "模型流在完成事件前结束。"))
}

fn take_sse_line(buffer: &mut Vec<u8>) -> Result<Option<String>, AgentError> {
    let Some(index) = buffer.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };
    let mut bytes = buffer.drain(..=index).collect::<Vec<_>>();
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| AgentError::new("llm_stream_invalid", "模型流返回了无效 UTF-8。"))
}

fn resolve_credentials(config: &LlmConfigInternal) -> Result<(String, String), AgentError> {
    let base = config
        .base_url
        .as_ref()
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 Base URL。"))?;
    let api_key = config
        .api_key
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 API Key。"))?;
    Ok((base, api_key))
}

pub async fn list_models(config: &LlmConfigInternal) -> Result<Vec<String>, AgentError> {
    let (base, api_key) = resolve_credentials(config)?;
    let response = reqwest::Client::new()
        .get(format!("{base}/models"))
        .bearer_auth(api_key)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AgentError::new(
            "llm_models_failed",
            format!("API 连接失败（{}）。", response.status()),
        ));
    }

    let value = response
        .json::<Value>()
        .await
        .map_err(|_| AgentError::new("llm_models_invalid", "API 返回了无效的模型列表。"))?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AgentError::new("llm_models_invalid", "API 返回了无效的模型列表。"))?;
    let mut seen = HashSet::new();
    Ok(items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert((*id).to_string()))
        .map(str::to_string)
        .collect())
}

pub async fn test_connection(
    config: &LlmConfigInternal,
) -> Result<(bool, String, Option<bool>), AgentError> {
    match complete(config, &[json!({"role": "user", "content": "ping"})], &[]).await {
        Ok(_) => {
            let image_supported = probe_image_support(config).await;
            Ok((true, "连接成功，对话测试通过。".into(), image_supported))
        }
        Err(error) => Ok((false, format!("对话失败：{}", error.message), None)),
    }
}

/// 用一张 1x1 透明 PNG 探测当前模型是否支持图片输入。
/// - 返回 `Some(true)`：多模态请求成功。
/// - 返回 `Some(false)`：命中 `llm_image_unsupported`。
/// - 返回 `None`：其它错误或异常，无法判定，跳过。
pub async fn probe_image_support(config: &LlmConfigInternal) -> Option<bool> {
    let probe = json!({
        "role": "user",
        "content": [
            { "type": "text", "text": "ping" },
            { "type": "image_url", "image_url": { "url": TINY_PNG_DATA_URL } }
        ]
    });
    match complete(config, &[probe], &[]).await {
        Ok(_) => Some(true),
        Err(error) if error.code == "llm_image_unsupported" => Some(false),
        Err(_) => None,
    }
}

/// 1x1 透明 PNG 的 data URL，用于探测图片输入支持。
const TINY_PNG_DATA_URL: &str =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

pub fn content_to_text(content: &Option<Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(|text| text.as_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

pub fn image_file_to_data_url(path: &str) -> Result<String, AgentError> {
    let bytes = std::fs::read(path).map_err(|error| {
        AgentError::new("capture_read_failed", format!("无法读取截屏文件：{error}"))
    })?;
    let lower = path.to_ascii_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "image/png"
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn tool_requests_apply_choice_and_explicitly_disable_parallel_calls() {
        let auto = request_body(
            "https://example.com/v1",
            "model",
            &[json!({"role": "user", "content": "hello"})],
            &[json!({"type": "function", "function": {"name": "tool"}})],
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap();
        let required = request_body(
            "https://example.com/v1",
            "model",
            &[json!({"role": "user", "content": "hello"})],
            &[json!({"type": "function", "function": {"name": "tool"}})],
            ToolChoiceMode::Required,
            true,
        )
        .unwrap();

        assert_eq!(auto["parallel_tool_calls"], Value::Bool(false));
        assert_eq!(auto["tool_choice"], Value::String("auto".into()));
        assert_eq!(required["parallel_tool_calls"], Value::Bool(false));
        assert_eq!(required["tool_choice"], Value::String("required".into()));
        assert!(required.get("thinking").is_none());
    }

    #[test]
    fn official_deepseek_v4_uses_thinking_only_when_tool_choice_is_not_required() {
        let auto = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &[json!({"role":"user","content":"approve"})],
            &[json!({"type":"function","function":{"name":"ask_user"}})],
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap();
        assert_eq!(auto["thinking"]["type"], "enabled");
        assert_eq!(auto["parallel_tool_calls"], false);
        assert!(auto.get("tool_choice").is_none());

        let required = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &[json!({"role":"user","content":"approve"})],
            &[json!({"type":"function","function":{"name":"ask_user"}})],
            ToolChoiceMode::Required,
            true,
        )
        .unwrap();
        assert_eq!(required["thinking"]["type"], "disabled");
        assert_eq!(required["tool_choice"], "required");
        assert!(is_official_deepseek_v4(
            "https://api.deepseek.com",
            "deepseek-v4-flash"
        ));
        assert!(!is_official_deepseek_v4(
            "https://example.com/v1",
            "deepseek-v4-pro"
        ));
        assert!(!is_official_deepseek_v4(
            "https://api.deepseek.com/v1",
            "other-model"
        ));
    }

    #[test]
    fn official_deepseek_v4_keeps_non_thinking_mode_after_forced_tool_call() {
        let messages = vec![
            json!({"role":"user","content":"finish the edit"}),
            json!({
                "role":"assistant",
                "content":"",
                "tool_calls":[{
                    "id":"call-1",
                    "type":"function",
                    "function":{"name":"get_editor_edit_result","arguments":"{}"}
                }]
            }),
            json!({"role":"tool","tool_call_id":"call-1","content":"done"}),
        ];
        let body = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &messages,
            &[json!({"type":"function","function":{"name":"get_editor_edit_result"}})],
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap();

        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["tool_choice"], "auto");
        assert!(body["messages"][1].get("reasoning_content").is_none());
    }

    #[test]
    fn deepseek_tool_history_replays_reasoning_content_and_non_null_content() {
        let messages = vec![
            json!({"role":"user","content":"检查编辑器"}),
            json!({
                "role":"assistant",
                "content":null,
                "reasoning_content":"需要读取编辑器状态。",
                "tool_calls":[{
                    "id":"snapshot-1",
                    "type":"function",
                    "function":{"name":"get_editor_snapshot","arguments":"{}"}
                }]
            }),
            json!({"role":"tool","tool_call_id":"snapshot-1","content":"{}"}),
        ];

        let body = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &messages,
            &[json!({"type":"function","function":{"name":"get_editor_snapshot"}})],
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap();

        assert_eq!(body["messages"][1]["content"], "");
        assert_eq!(
            body["messages"][1]["reasoning_content"],
            "需要读取编辑器状态。"
        );
        assert_eq!(
            body["messages"][1]["tool_calls"][0]["function"]["name"],
            "get_editor_snapshot"
        );
    }

    #[test]
    fn deepseek_falls_back_to_non_thinking_for_tool_history_without_reasoning_content() {
        let messages = vec![
            json!({"role":"assistant","content":"","tool_calls":[{
                "id":"snapshot-1",
                "type":"function",
                "function":{"name":"get_editor_snapshot","arguments":"{}"}
            }]}),
            json!({"role":"tool","tool_call_id":"snapshot-1","content":"{}"}),
        ];

        let tools = [json!({"type":"function","function":{"name":"get_editor_snapshot"}})];
        let error = normalize_deepseek_history(&messages).unwrap_err();
        assert_eq!(error.code, "llm_reasoning_history_incomplete");

        let body = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &messages,
            &tools,
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap();

        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["tool_choice"], "auto");
        assert!(request_body(
            "https://example.com/v1",
            "deepseek-v4-pro",
            &messages,
            &tools,
            ToolChoiceMode::Auto,
            true,
        )
        .is_ok());
    }

    #[derive(Clone)]
    struct MockHttpResponse {
        status: u16,
        content_type: &'static str,
        body: String,
    }

    async fn spawn_mock_http_recording(
        responses: Vec<MockHttpResponse>,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let bodies = Arc::new(Mutex::new(responses));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = requests.clone();
        tokio::spawn(async move {
            let mut index = 0usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 65536];
                let size = socket.read(&mut buf).await.unwrap_or(0);
                recorded_requests
                    .lock()
                    .await
                    .push(String::from_utf8_lossy(&buf[..size]).to_string());
                let reply = {
                    let list = bodies.lock().await;
                    list.get(index)
                        .cloned()
                        .unwrap_or_else(|| MockHttpResponse {
                            status: 200,
                            content_type: "text/event-stream",
                            body: r#"data: {"choices":[{"delta":{"content":"done"}}]}

data: [DONE]
"#
                            .into(),
                        })
                };
                index += 1;
                let reason = if reply.status == 200 { "OK" } else { "Error" };
                let response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    reason,
                    reply.content_type,
                    reply.body.len(),
                    reply.body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{addr}/v1"), requests)
    }

    async fn spawn_mock_http(responses: Vec<MockHttpResponse>) -> String {
        spawn_mock_http_recording(responses).await.0
    }

    async fn spawn_mock_llm(responses: Vec<String>) -> String {
        spawn_mock_http(
            responses
                .into_iter()
                .map(|body| MockHttpResponse {
                    status: 200,
                    content_type: "text/event-stream",
                    body,
                })
                .collect(),
        )
        .await
    }

    #[tokio::test]
    async fn list_models_sanitizes_ids_without_chat_request() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 200,
            content_type: "application/json",
            body: r#"{"data":[{"id":" mock-model "},{"id":""},{"id":"mock-mini"},{"id":"mock-model"},{"name":"ignored"}]}"#.into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: None,
            context_window: None,
            max_input_tokens: None,
        };

        let models = list_models(&config).await.unwrap();

        assert_eq!(
            models,
            vec!["mock-model".to_string(), "mock-mini".to_string()]
        );
    }

    #[tokio::test]
    async fn list_models_rejects_http_and_malformed_responses() {
        for reply in [
            MockHttpResponse {
                status: 401,
                content_type: "application/json",
                body: r#"{"error":"secret provider detail"}"#.into(),
            },
            MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: r#"{"models":[]}"#.into(),
            },
            MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: "not-json".into(),
            },
        ] {
            let base_url = spawn_mock_http(vec![reply]).await;
            let config = LlmConfigInternal {
                api_mode: LlmApiMode::ChatCompletions,
                base_url: Some(base_url),
                api_key: Some("test-key".into()),
                model: None,
                context_window: None,
                max_input_tokens: None,
            };

            let error = list_models(&config).await.unwrap_err();

            assert!(matches!(
                error.code.as_str(),
                "llm_models_failed" | "llm_models_invalid"
            ));
            assert!(!error.message.contains("secret provider detail"));
        }
    }

    #[tokio::test]
    async fn list_models_accepts_a_valid_empty_list() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 200,
            content_type: "application/json",
            body: r#"{"data":[]}"#.into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: None,
            context_window: None,
            max_input_tokens: None,
        };

        assert!(list_models(&config).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_connection_runs_short_chat_when_model_configured() {
        let base_url = spawn_mock_http(vec![
            MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}]}"#.into(),
            },
            MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}]}"#.into(),
            },
        ])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let (ok, message, image_supported) = test_connection(&config).await.unwrap();
        assert!(ok);
        assert_eq!(message, "连接成功，对话测试通过。");
        assert_eq!(image_supported, Some(true));
    }

    #[tokio::test]
    async fn test_connection_fails_when_chat_fails() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 500,
            content_type: "application/json",
            body: r#"{"error":"boom"}"#.into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let (ok, message, image_supported) = test_connection(&config).await.unwrap();
        assert!(!ok);
        assert!(message.starts_with("对话失败："));
        assert_eq!(image_supported, None);
    }

    #[test]
    fn non_streaming_tool_call_preserves_reasoning_content() {
        let parsed = serde_json::from_value(json!({
            "choices":[{
                "message":{
                    "role":"assistant",
                    "content":null,
                    "reasoning_content":"先核对预览，再请求批准。",
                    "tool_calls":[{
                        "id":"ask-1",
                        "type":"function",
                        "function":{"name":"ask_user","arguments":"{\"question\":\"继续？\"}"}
                    }]
                }
            }]
        }))
        .unwrap();
        let message = first_message(parsed).unwrap();

        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("先核对预览，再请求批准。")
        );
        assert_eq!(message.tool_calls.unwrap()[0].function.name, "ask_user");
    }

    #[tokio::test]
    async fn streaming_react_loop_parses_tool_then_final() {
        let tool_sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_editor_snapshot","arguments":"{}"}}]}}]}

data: [DONE]
"#;
        let final_sse = r#"data: {"choices":[{"delta":{"content":"截屏"}}]}

data: {"choices":[{"delta":{"content":"完成"}}]}

data: [DONE]
"#;
        let base_url = spawn_mock_llm(vec![tool_sse.into(), final_sse.into()]).await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let first = chat_completions_stream(
            &config,
            &[json!({"role":"user","content":"看一下编辑器"})],
            &[json!({"type":"function","function":{"name":"get_editor_snapshot"}})],
            ToolChoiceMode::Auto,
            |_| {},
        )
        .await
        .unwrap();
        assert!(first.tool_calls.as_ref().unwrap()[0].function.name == "get_editor_snapshot");
        assert!(first.reasoning_content.is_none());

        let mut deltas = String::new();
        let second = chat_completions_stream(
            &config,
            &[
                json!({"role":"user","content":"看一下编辑器"}),
                json!({"role":"assistant","tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_editor_snapshot","arguments":"{}"}}]}),
                json!({"role":"tool","tool_call_id":"call_1","content":"{}"}),
            ],
            &[],
            ToolChoiceMode::Auto,
            |delta| {
                if let ChatStreamDelta::Text(piece) = delta {
                    deltas.push_str(&piece);
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(content_to_text(&second.content), "截屏完成");
        assert_eq!(deltas, "截屏完成");
        assert!(second.reasoning_content.is_none());
    }

    #[tokio::test]
    async fn streaming_tool_calls_report_accumulated_ask_arguments() {
        let chunks = [
            json!({"choices":[{"delta":{"reasoning_content":"需要先"}}]}),
            json!({"choices":[{"delta":{"reasoning_content":"获得批准。"}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"ask-1","type":"function","function":{"name":"ask_user","arguments":"{\"question\":\"## 计划\\n"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"- 核对参数\\n- 执行调整\""}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":",\"options\":[\"继续\"]}"}}]}}]}),
        ];
        let body = chunks
            .into_iter()
            .map(|payload| format!("data: {payload}\n\n"))
            .collect::<String>()
            + "data: [DONE]\n";
        let base_url = spawn_mock_llm(vec![body]).await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        let mut snapshots = Vec::new();

        let message = chat_completions_stream(
            &config,
            &[json!({"role":"user","content":"调整参数"})],
            &[json!({"type":"function","function":{"name":"ask_user"}})],
            ToolChoiceMode::Required,
            |delta| {
                if let ChatStreamDelta::ToolCall {
                    name, arguments, ..
                } = delta
                {
                    snapshots.push((name, arguments));
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].0, "ask_user");
        assert!(snapshots[0].1.ends_with("## 计划\\n"));
        assert!(snapshots[1].1.contains("执行调整"));
        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("需要先获得批准。")
        );
        let call = &message.tool_calls.unwrap()[0];
        assert_eq!(call.function.name, "ask_user");
        assert_eq!(
            serde_json::from_str::<Value>(&call.function.arguments).unwrap()["options"][0],
            "继续"
        );
    }

    fn request_json(request: &str) -> Value {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    #[test]
    fn responses_body_maps_images_and_function_tools() {
        let body = responses_request_body(
            "mock-model",
            &[json!({
                "role": "user",
                "content": [
                    { "type": "text", "text": "查看" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,AA==" } }
                ]
            })],
            &[json!({
                "type": "function",
                "function": {
                    "name": "inspect",
                    "description": "Inspect",
                    "parameters": {
                        "type": "object",
                        "properties": { "target": { "type": "string" } },
                        "required": ["target"]
                    }
                }
            })],
            ToolChoiceMode::Required,
            true,
        )
        .unwrap();

        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["parallel_tool_calls"], false);
        assert_eq!(body["tool_choice"], "required");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][0]["content"][1]["type"], "input_image");
        assert_eq!(
            body["input"][0]["content"][1]["image_url"],
            "data:image/png;base64,AA=="
        );
        assert_eq!(body["tools"][0]["name"], "inspect");
        assert_eq!(body["tools"][0]["strict"], false);
        assert!(body["tools"][0].get("function").is_none());
    }

    #[tokio::test]
    async fn responses_connection_test_uses_responses_for_text_and_image() {
        let reply = MockHttpResponse {
            status: 200,
            content_type: "application/json",
            body: r#"{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"pong"}]}]}"#.into(),
        };
        let (base_url, requests) = spawn_mock_http_recording(vec![reply.clone(), reply]).await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::Responses,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let (ok, _, image_supported) = test_connection(&config).await.unwrap();

        assert!(ok);
        assert_eq!(image_supported, Some(true));
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.starts_with("POST /v1/responses ")));
        assert_eq!(request_json(&requests[0])["store"], false);
        assert_eq!(
            request_json(&requests[1])["input"][0]["content"][1]["type"],
            "input_image"
        );
    }

    #[tokio::test]
    async fn responses_stream_replays_reasoning_call_and_tool_output_locally() {
        let tool_sse = r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","encrypted_content":"encrypted"}}

data: {"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_1","encrypted_content":"encrypted"}}

data: {"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"inspect","arguments":""}}

data: {"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"target\":"}

data: {"type":"response.function_call_arguments.delta","output_index":1,"delta":"\"editor\"}"}

data: {"type":"response.function_call_arguments.done","output_index":1,"arguments":"{\"target\":\"editor\"}"}

data: {"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"inspect","arguments":"{\"target\":\"editor\"}"}}

data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"reasoning","id":"rs_1","encrypted_content":"encrypted"},{"type":"function_call","id":"fc_1","call_id":"call_1","name":"inspect","arguments":"{\"target\":\"editor\"}"}]}}

"#;
        let final_sse = r#"data: {"type":"response.output_text.delta","delta":"检查完成"}

data: {"type":"response.output_item.done","output_index":0,"item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"检查完成"}]}}

data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"检查完成"}]}]}}

"#;
        let (base_url, requests) = spawn_mock_http_recording(vec![
            MockHttpResponse {
                status: 200,
                content_type: "text/event-stream",
                body: tool_sse.into(),
            },
            MockHttpResponse {
                status: 200,
                content_type: "text/event-stream",
                body: final_sse.into(),
            },
        ])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::Responses,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        let tool = json!({
            "type": "function",
            "function": {
                "name": "inspect",
                "description": "Inspect",
                "parameters": { "type": "object", "properties": {} }
            }
        });
        let mut tool_deltas = Vec::new();
        let first = complete_stream(
            &config,
            &[json!({"role":"user","content":"检查"})],
            &[tool],
            ToolChoiceMode::Auto,
            |delta| {
                if let ChatStreamDelta::ToolCall { arguments, .. } = delta {
                    tool_deltas.push(arguments);
                }
            },
        )
        .await
        .unwrap();
        let call = first.tool_calls.as_ref().unwrap().first().unwrap();
        assert_eq!(call.id, "call_1");
        assert_eq!(call.function.name, "inspect");
        assert_eq!(call.function.arguments, r#"{"target":"editor"}"#);
        assert_eq!(tool_deltas.last().unwrap(), r#"{"target":"editor"}"#);

        let mut assistant = json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.function.name,
                    "arguments": call.function.arguments,
                }
            }],
        });
        assistant["__responses_output"] = Value::Array(first.response_output.clone().unwrap());
        let mut text = String::new();
        let final_message = complete_stream(
            &config,
            &[
                json!({"role":"user","content":"检查"}),
                assistant,
                json!({"role":"tool","tool_call_id":"call_1","content":"{\"ok\":true}"}),
            ],
            &[],
            ToolChoiceMode::Auto,
            |delta| {
                if let ChatStreamDelta::Text(piece) = delta {
                    text.push_str(&piece);
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(text, "检查完成");
        assert_eq!(content_to_text(&final_message.content), "检查完成");

        let requests = requests.lock().await;
        let second = request_json(&requests[1]);
        let input = second["input"].as_array().unwrap();
        assert!(input.iter().any(|item| {
            item["type"] == "reasoning" && item["encrypted_content"] == "encrypted"
        }));
        assert!(input
            .iter()
            .any(|item| item["type"] == "function_call" && item["call_id"] == "call_1"));
        assert!(input
            .iter()
            .any(|item| { item["type"] == "function_call_output" && item["call_id"] == "call_1" }));
    }

    #[tokio::test]
    async fn responses_stream_rejects_eof_before_completion() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 200,
            content_type: "text/event-stream",
            body: "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n"
                .into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::Responses,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let error = complete_stream(
            &config,
            &[json!({"role":"user","content":"hi"})],
            &[],
            ToolChoiceMode::Auto,
            |_| {},
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, "llm_stream_incomplete");
    }

    #[tokio::test]
    async fn responses_http_error_does_not_expose_provider_body() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 500,
            content_type: "application/json",
            body: r#"{"error":"secret provider detail"}"#.into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::Responses,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };

        let error = complete(&config, &[json!({"role":"user","content":"hi"})], &[])
            .await
            .unwrap_err();

        assert_eq!(error.code, "llm_request_failed");
        assert!(!error.message.contains("secret provider detail"));
    }

    #[test]
    fn sse_line_buffer_waits_for_complete_utf8() {
        let source = "data: {\"text\":\"计划\"}\n".as_bytes();
        let marker = "计".as_bytes();
        let start = source
            .windows(marker.len())
            .position(|window| window == marker)
            .unwrap();
        let split = start + 1;
        let mut buffer = source[..split].to_vec();

        assert!(take_sse_line(&mut buffer).unwrap().is_none());
        buffer.extend_from_slice(&source[split..]);

        assert_eq!(
            take_sse_line(&mut buffer).unwrap().as_deref(),
            Some("data: {\"text\":\"计划\"}")
        );
    }

    #[test]
    fn image_data_url_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nbc-img-{}", crate::agent::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.png");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&[137, 80, 78, 71, 13, 10, 26, 10]).unwrap();
        let url = image_file_to_data_url(path.to_str().unwrap()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn detect_image_unsupported_matches_provider_bodies() {
        let positive = [
            r#"{"error":{"message":"image_url is not supported by this model"}}"#,
            "Unsupported value: 'messages.[0].content.[0].image_url.url' does not exist",
            "The model does not support image input.",
            "image input is not supported",
            "vision is not available for this model",
            "multimodal is not supported",
            "Invalid image_url",
        ];
        for body in positive {
            assert!(detect_image_unsupported(body), "should match: {body}");
        }
    }

    #[test]
    fn detect_image_unsupported_ignores_unrelated_errors() {
        let negative = [
            r#"{"error":{"message":"rate limit exceeded"}}"#,
            "context length exceeded",
            "invalid api key",
            "model not found",
            "internal server error",
            "",
        ];
        for body in negative {
            assert!(!detect_image_unsupported(body), "should not match: {body}");
        }
    }

    #[tokio::test]
    async fn chat_completions_classifies_image_unsupported_error() {
        let base_url = spawn_mock_http(vec![MockHttpResponse {
            status: 400,
            content_type: "application/json",
            body: r#"{"error":{"message":"image_url is not supported by this model"}}"#.into(),
        }])
        .await;
        let config = LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        };
        let error = chat_completions(&config, &[json!({"role":"user","content":"hi"})], &[])
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm_image_unsupported");
    }

    fn mock_chat_config(base_url: String) -> LlmConfigInternal {
        LlmConfigInternal {
            api_mode: LlmApiMode::ChatCompletions,
            base_url: Some(base_url),
            api_key: Some("test-key".into()),
            model: Some("mock-model".into()),
            context_window: None,
            max_input_tokens: None,
        }
    }

    fn json_http(status: u16, body: &str) -> MockHttpResponse {
        MockHttpResponse {
            status,
            content_type: "application/json",
            body: body.into(),
        }
    }

    #[tokio::test]
    async fn chat_completions_retries_http_400() {
        let hi = [json!({"role":"user","content":"hi"})];

        let (base_url, requests) = spawn_mock_http_recording(vec![
            json_http(400, r#"{"error":{"message":"temporary"}}"#),
            json_http(200, r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}]}"#),
        ])
        .await;
        let message = chat_completions(&mock_chat_config(base_url), &hi, &[])
            .await
            .unwrap();
        assert_eq!(content_to_text(&message.content), "ok");
        assert_eq!(requests.lock().await.len(), 2);

        let replies = (0..6)
            .map(|_| json_http(400, r#"{"error":{"message":"temporary"}}"#))
            .collect();
        let (base_url, requests) = spawn_mock_http_recording(replies).await;
        let error = chat_completions(&mock_chat_config(base_url), &hi, &[])
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm_request_failed");
        assert_eq!(requests.lock().await.len(), 6);

        let (base_url, requests) =
            spawn_mock_http_recording(vec![json_http(500, r#"{"error":{"message":"internal"}}"#)])
                .await;
        let error = chat_completions(&mock_chat_config(base_url), &hi, &[])
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm_request_failed");
        assert_eq!(requests.lock().await.len(), 1);

        let (base_url, requests) = spawn_mock_http_recording(vec![json_http(
            400,
            r#"{"error":{"message":"image_url is not supported by this model"}}"#,
        )])
        .await;
        let error = chat_completions(&mock_chat_config(base_url), &hi, &[])
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm_image_unsupported");
        assert_eq!(requests.lock().await.len(), 1);
    }
}
