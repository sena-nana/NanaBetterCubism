use crate::agent::store::LlmConfigInternal;
use crate::agent::AgentError;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

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
    let deepseek_v4 = is_official_deepseek_v4(base_url, model);
    let messages = if deepseek_v4 {
        normalize_deepseek_history(messages)?
    } else {
        messages.to_vec()
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
            body["thinking"] = json!({ "type": "enabled" });
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
            format!("当前模型不支持图片输入 ({status}): {text}"),
        )
    } else {
        AgentError::new(
            "llm_request_failed",
            format!("模型请求失败 ({status}): {text}"),
        )
    }
}

pub async fn chat_completions(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
) -> Result<ChatMessagePayload, AgentError> {
    let (base, api_key, model) = resolve_endpoint(config)?;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&request_body(
            &base,
            &model,
            messages,
            tools,
            ToolChoiceMode::Auto,
            false,
        )?)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(classify_request_failure(status, text));
    }
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
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(&api_key)
        .json(&request_body(
            &base,
            &model,
            messages,
            tools,
            tool_choice,
            true,
        )?)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(classify_request_failure(status, text));
    }

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
    })
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
    match chat_completions(config, &[json!({"role": "user", "content": "ping"})], &[]).await {
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
    match chat_completions(config, &[probe], &[]).await {
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
    fn official_deepseek_v4_uses_thinking_without_tool_choice() {
        let body = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &[json!({"role":"user","content":"approve"})],
            &[json!({"type":"function","function":{"name":"ask_user"}})],
            ToolChoiceMode::Required,
            true,
        )
        .unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["parallel_tool_calls"], false);
        assert!(body.get("tool_choice").is_none());
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
    fn deepseek_rejects_tool_history_without_reasoning_content() {
        let messages = vec![
            json!({"role":"assistant","content":"","tool_calls":[{
                "id":"snapshot-1",
                "type":"function",
                "function":{"name":"get_editor_snapshot","arguments":"{}"}
            }]}),
            json!({"role":"tool","tool_call_id":"snapshot-1","content":"{}"}),
        ];

        let tools = [json!({"type":"function","function":{"name":"get_editor_snapshot"}})];
        let error = request_body(
            "https://api.deepseek.com/v1",
            "deepseek-v4-pro",
            &messages,
            &tools,
            ToolChoiceMode::Auto,
            true,
        )
        .unwrap_err();

        assert_eq!(error.code, "llm_reasoning_history_incomplete");
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

    async fn spawn_mock_http(responses: Vec<MockHttpResponse>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let bodies = Arc::new(Mutex::new(responses));
        tokio::spawn(async move {
            let mut index = 0usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 65536];
                let _ = socket.read(&mut buf).await;
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
        format!("http://{addr}/v1")
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
}
