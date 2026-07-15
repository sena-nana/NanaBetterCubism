use crate::domain::EDIT_API_VERSION;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct RpcEvent {
    pub method: String,
    pub data: Value,
}

#[derive(Debug)]
pub enum RpcError {
    Connection(String),
    Disconnected,
    Timeout,
    Protocol(String),
    Editor { kind: String },
}

impl RpcError {
    pub fn is_transport_failure(&self) -> bool {
        matches!(
            self,
            Self::Connection(_) | Self::Disconnected | Self::Timeout
        )
    }

    pub fn editor_kind(&self) -> Option<&str> {
        match self {
            Self::Editor { kind } => Some(kind),
            _ => None,
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(message) => write!(formatter, "连接失败：{message}"),
            Self::Disconnected => write!(formatter, "Editor 连接已断开"),
            Self::Timeout => write!(formatter, "Editor 请求超时"),
            Self::Protocol(message) => write!(formatter, "Editor 协议错误：{message}"),
            Self::Editor { kind } => write!(formatter, "Editor 返回错误：{kind}"),
        }
    }
}

enum RpcCommand {
    Request {
        method: String,
        data: Value,
        response: oneshot::Sender<Result<Value, RpcError>>,
    },
    Close,
}

#[derive(Clone)]
pub struct RpcClient {
    commands: mpsc::Sender<RpcCommand>,
    events: broadcast::Sender<RpcEvent>,
    history: Arc<Mutex<VecDeque<RpcEvent>>>,
}

impl RpcClient {
    pub async fn connect(port: u16) -> Result<Self, RpcError> {
        let url = format!("ws://127.0.0.1:{port}");
        let (socket, _) = connect_async(&url)
            .await
            .map_err(|error| RpcError::Connection(error.to_string()))?;
        let (command_tx, mut command_rx) = mpsc::channel::<RpcCommand>(64);
        let (event_tx, _) = broadcast::channel::<RpcEvent>(64);
        let actor_events = event_tx.clone();
        let history = Arc::new(Mutex::new(VecDeque::<RpcEvent>::with_capacity(64)));
        let actor_history = history.clone();

        tokio::spawn(async move {
            let mut socket = socket;
            let mut pending: HashMap<String, oneshot::Sender<Result<Value, RpcError>>> =
                HashMap::new();

            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        match command {
                            Some(RpcCommand::Request { method, data, response }) => {
                                let request_id = Uuid::new_v4().simple().to_string();
                                let envelope = json!({
                                    "Version": EDIT_API_VERSION,
                                    "RequestId": request_id,
                                    "Type": "Request",
                                    "Method": method,
                                    "Data": data,
                                });
                                let serialized = match serde_json::to_string(&envelope) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        let _ = response.send(Err(RpcError::Protocol(error.to_string())));
                                        continue;
                                    }
                                };
                                if socket.send(Message::Text(serialized.into())).await.is_err() {
                                    let _ = response.send(Err(RpcError::Disconnected));
                                    break;
                                }
                                pending.insert(request_id, response);
                            }
                            Some(RpcCommand::Close) | None => {
                                let _ = socket.close(None).await;
                                break;
                            }
                        }
                    }
                    message = socket.next() => {
                        let Some(message) = message else { break };
                        let message = match message {
                            Ok(Message::Text(value)) => value.to_string(),
                            Ok(Message::Binary(value)) => match String::from_utf8(value.to_vec()) {
                                Ok(value) => value,
                                Err(_) => continue,
                            },
                            Ok(Message::Close(_)) => break,
                            Ok(_) => continue,
                            Err(_) => break,
                        };
                        let envelope: Value = match serde_json::from_str(&message) {
                            Ok(value) => value,
                            Err(_) => continue,
                        };
                        let message_type = envelope.get("Type").and_then(Value::as_str).unwrap_or_default();
                        let method = envelope.get("Method").and_then(Value::as_str).unwrap_or_default();
                        let data = envelope.get("Data").cloned().unwrap_or_else(|| json!({}));

                        if message_type == "Event" {
                            let event = RpcEvent { method: method.into(), data };
                            {
                                let mut history = actor_history.lock().await;
                                if history.len() == 64 {
                                    history.pop_front();
                                }
                                history.push_back(event.clone());
                            }
                            let _ = actor_events.send(event);
                            continue;
                        }

                        let Some(request_id) = envelope.get("RequestId").and_then(Value::as_str) else {
                            continue;
                        };
                        let Some(response) = pending.remove(request_id) else { continue };
                        let result = if message_type == "Response" {
                            Ok(data)
                        } else if message_type == "Error" {
                            let kind = data
                                .get("ErrorType")
                                .and_then(Value::as_str)
                                .unwrap_or("UnknownEditorError")
                                .to_string();
                            Err(RpcError::Editor { kind })
                        } else {
                            Err(RpcError::Protocol(format!("未知响应类型 {message_type}")))
                        };
                        let _ = response.send(result);
                    }
                }
            }

            for (_, response) in pending.drain() {
                let _ = response.send(Err(RpcError::Disconnected));
            }
            let _ = actor_events.send(RpcEvent {
                method: "__Disconnected".into(),
                data: json!({}),
            });
        });

        Ok(Self {
            commands: command_tx,
            events: event_tx,
            history,
        })
    }

    pub async fn request(&self, method: &str, data: Value) -> Result<Value, RpcError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(RpcCommand::Request {
                method: method.into(),
                data,
                response: response_tx,
            })
            .await
            .map_err(|_| RpcError::Disconnected)?;
        tokio::time::timeout(REQUEST_TIMEOUT, response_rx)
            .await
            .map_err(|_| RpcError::Timeout)?
            .map_err(|_| RpcError::Disconnected)?
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RpcEvent> {
        self.events.subscribe()
    }

    pub async fn recent_events(&self) -> Vec<RpcEvent> {
        self.history.lock().await.iter().cloned().collect()
    }

    pub async fn close(&self) {
        let _ = self.commands.send(RpcCommand::Close).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    async fn mock_server(response_type: &'static str, response_data: Value) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let response = json!({
                "Version": EDIT_API_VERSION,
                "RequestId": request["RequestId"],
                "Type": response_type,
                "Method": request["Method"],
                "Data": response_data,
            });
            socket
                .send(Message::Text(response.to_string().into()))
                .await
                .unwrap();
        });
        port
    }

    #[tokio::test]
    async fn correlates_response_by_request_id() {
        let port = mock_server("Response", json!({"Result": true})).await;
        let client = RpcClient::connect(port).await.unwrap();
        let response = client.request("GetIsApproval", json!({})).await.unwrap();
        assert_eq!(response["Result"], true);
    }

    #[tokio::test]
    async fn preserves_editor_error_kind() {
        let port = mock_server("Error", json!({"ErrorType": "UnsupportedVersion"})).await;
        let client = RpcClient::connect(port).await.unwrap();
        let error = client
            .request("GetIsEditApproval", json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.editor_kind(), Some("UnsupportedVersion"));
    }

    #[tokio::test]
    async fn retains_official_events_for_domain_consumers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket
                .send(Message::Text(
                    json!({
                        "Version": EDIT_API_VERSION,
                        "Type": "Event",
                        "Method": "NotifyChangeEditMode",
                        "Data": {"EditMode": "Modeling"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        });
        let client = RpcClient::connect(port).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let events = client.recent_events().await;
                if !events.is_empty() {
                    assert_eq!(events[0].method, "NotifyChangeEditMode");
                    assert_eq!(events[0].data["EditMode"], "Modeling");
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn classifies_transport_failures() {
        assert!(RpcError::Connection("refused".into()).is_transport_failure());
        assert!(RpcError::Disconnected.is_transport_failure());
        assert!(RpcError::Timeout.is_transport_failure());
        assert!(!RpcError::Protocol("invalid response".into()).is_transport_failure());
        assert!(!RpcError::Editor {
            kind: "InvalidData".into(),
        }
        .is_transport_failure());
    }
}
