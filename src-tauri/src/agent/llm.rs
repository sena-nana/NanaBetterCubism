use crate::agent::store::LlmConfigInternal;
use crate::agent::AgentError;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

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

#[derive(Debug, Default)]
struct StreamingToolCall {
    id: String,
    r#type: Option<String>,
    name: String,
    arguments: String,
}

fn resolve_endpoint(
    config: &LlmConfigInternal,
) -> Result<(String, String, String), AgentError> {
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

fn request_body(model: &str, messages: &[Value], tools: &[Value], stream: bool) -> Value {
    if tools.is_empty() {
        json!({ "model": model, "messages": messages, "stream": stream })
    } else {
        json!({
            "model": model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "stream": stream,
        })
    }
}

fn first_message(parsed: ChatCompletionResponse) -> Result<ChatMessagePayload, AgentError> {
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message)
        .ok_or_else(|| AgentError::new("llm_empty", "模型未返回内容。"))
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
        .json(&request_body(&model, messages, tools, false))
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AgentError::new(
            "llm_request_failed",
            format!("模型请求失败 ({status}): {text}"),
        ));
    }
    first_message(response.json().await?)
}

pub async fn chat_completions_stream<F>(
    config: &LlmConfigInternal,
    messages: &[Value],
    tools: &[Value],
    mut on_delta: F,
) -> Result<ChatMessagePayload, AgentError>
where
    F: FnMut(&str),
{
    let (base, api_key, model) = resolve_endpoint(config)?;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(&api_key)
        .json(&request_body(&model, messages, tools, true))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AgentError::new(
            "llm_request_failed",
            format!("模型请求失败 ({status}): {text}"),
        ));
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
            on_delta(&text);
        }
        return Ok(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut tool_calls: BTreeMap<u64, StreamingToolCall> = BTreeMap::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buffer.find('\n') {
            let mut line = buffer[..idx].to_string();
            buffer.drain(..=idx);
            if line.ends_with('\r') {
                line.pop();
            }
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
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
                    on_delta(piece);
                }
            }
            if let Some(calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
                for call in calls {
                    let index = call.get("index").and_then(|value| value.as_u64()).unwrap_or(0);
                    let entry = tool_calls.entry(index).or_default();
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
                            entry.name.push_str(name);
                        }
                        if let Some(arguments) =
                            function.get("arguments").and_then(|value| value.as_str())
                        {
                            entry.arguments.push_str(arguments);
                        }
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
        tool_calls,
    })
}

pub async fn test_connection(
    config: &LlmConfigInternal,
) -> Result<(bool, String, Vec<String>), AgentError> {
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
        .ok_or_else(|| AgentError::new("llm_not_configured", "请先配置 API Key。"))?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{base}/models"))
        .bearer_auth(api_key)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Ok((false, format!("连接失败 ({status}): {text}"), Vec::new()));
    }

    let value: Value = response.json().await.unwrap_or(json!({}));
    let models = value
        .get("data")
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let matched = config
        .model
        .as_ref()
        .map(|model| models.iter().any(|item| item == model))
        .unwrap_or(true);

    let message = if models.is_empty() {
        "已连接（端点未返回 /models 列表）。".into()
    } else if matched {
        format!("已连接，发现 {} 个模型。", models.len())
    } else {
        format!(
            "已连接，但当前模型不在列表中（共 {} 个可用）。",
            models.len()
        )
    };

    Ok((true, message, models))
}

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
        AgentError::new(
            "capture_read_failed",
            format!("无法读取截屏文件：{error}"),
        )
    })?;
    let mime = if path.to_ascii_lowercase().ends_with(".jpg")
        || path.to_ascii_lowercase().ends_with(".jpeg")
    {
        "image/jpeg"
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

    async fn spawn_mock_llm(responses: Vec<String>) -> String {
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
                let body = {
                    let list = bodies.lock().await;
                    list.get(index).cloned().unwrap_or_else(|| {
                        r#"data: {"choices":[{"delta":{"content":"done"}}]}

data: [DONE]
"#
                        .into()
                    })
                };
                index += 1;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{addr}/v1")
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
        };

        let first = chat_completions_stream(
            &config,
            &[json!({"role":"user","content":"看一下编辑器"})],
            &[json!({"type":"function","function":{"name":"get_editor_snapshot"}})],
            |_| {},
        )
        .await
        .unwrap();
        assert!(first.tool_calls.as_ref().unwrap()[0].function.name == "get_editor_snapshot");

        let mut deltas = String::new();
        let second = chat_completions_stream(
            &config,
            &[
                json!({"role":"user","content":"看一下编辑器"}),
                json!({"role":"assistant","tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_editor_snapshot","arguments":"{}"}}]}),
                json!({"role":"tool","tool_call_id":"call_1","content":"{}"}),
            ],
            &[],
            |piece| deltas.push_str(piece),
        )
        .await
        .unwrap();
        assert_eq!(content_to_text(&second.content), "截屏完成");
        assert_eq!(deltas, "截屏完成");
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
}
