use crate::agent::images::ChatImageAttachment;
use crate::agent::plan::{PendingPlanApproval, PlanApprovalAction, PlanDocument};
use crate::agent::store::{AgentBackendConfigView, AgentStore, PendingQuestion, PlanStep};
use crate::agent::{emit_conversations_changed, new_id, AgentError, AgentRuntime, AgentTurnMode};
use crate::mcp::{start_internal_server, InternalMcpServer};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch, Mutex, RwLock};
use tokio::time::{sleep, timeout};

const MCP_SERVER_NAME: &str = "nanabettercubism";
const MCP_TOKEN_ENV: &str = "NANABETTERCUBISM_CODEX_MCP_TOKEN";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TURN_EVENT_POLL: Duration = Duration::from_millis(100);
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_DIAGNOSTICS: usize = 20;

const CODEX_DEVELOPER_INSTRUCTIONS: &str = r#"
You are the NanaBetterCubism assistant. Work only through the nanabettercubism MCP server.
Do not use shell commands, file operations, web search, browser/computer use, apps, plugins,
skills, image generation, subagents, or any tool outside that MCP server.
All Cubism model writes must follow preview, confirmed transaction, result query, and semantic
reread verification. Never report commit or rollback when the result is unknown.
PSD and project memory are scoped to the current NanaBetterCubism conversation.
"#;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexStatusState {
    #[default]
    Unavailable,
    Starting,
    AuthRequired,
    Ready,
    Incompatible,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub state: CodexStatusState,
    pub version: Option<String>,
    pub model: Option<String>,
    pub image_input_supported: Option<bool>,
    pub plan_supported: bool,
    pub message: String,
}

struct PendingCodexAnswer {
    sender: oneshot::Sender<String>,
    conversation_id: String,
    generation: u64,
    turn_id: String,
}

#[derive(Debug, PartialEq, Eq)]
enum PendingAnswerResult {
    Answer(String),
    Cancelled,
    TransportClosed,
    Expired,
}

#[derive(Default)]
struct TurnRunState {
    turn_id: Option<String>,
    started: bool,
    terminal: bool,
}

pub struct CodexManager {
    data_dir: PathBuf,
    sessions: Mutex<HashMap<String, Arc<CodexSession>>>,
    pending_answers: Mutex<HashMap<String, PendingCodexAnswer>>,
    status: RwLock<CodexStatus>,
    next_generation: AtomicU64,
}

impl CodexManager {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.unwrap_or_else(std::env::temp_dir),
            sessions: Mutex::new(HashMap::new()),
            pending_answers: Mutex::new(HashMap::new()),
            status: RwLock::new(CodexStatus {
                message: "尚未检测本地 Codex。".into(),
                ..CodexStatus::default()
            }),
            next_generation: AtomicU64::new(1),
        }
    }

    pub async fn status(&self) -> CodexStatus {
        self.status.read().await.clone()
    }

    pub async fn probe(&self, config: &AgentBackendConfigView) -> CodexStatus {
        let starting = CodexStatus {
            state: CodexStatusState::Starting,
            message: "正在检测本地 Codex…".into(),
            ..CodexStatus::default()
        };
        *self.status.write().await = starting;

        let status = match probe_codex(config).await {
            Ok(status) => status,
            Err(error) => status_from_error(error),
        };
        *self.status.write().await = status.clone();
        status
    }

    pub async fn answer_question(
        &self,
        action_id: &str,
        answer: String,
    ) -> Result<bool, AgentError> {
        let pending = self.pending_answers.lock().await.remove(action_id);
        let Some(pending) = pending else {
            return Ok(false);
        };
        pending
            .sender
            .send(answer)
            .map_err(|_| AgentError::new("codex_question_expired", "Codex 提问已失效。"))?;
        Ok(true)
    }

    pub async fn run_turn(
        self: &Arc<Self>,
        app: AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: String,
        mode: AgentTurnMode,
        text: String,
        attachments: Vec<ChatImageAttachment>,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), AgentError> {
        let result = self
            .run_turn_inner(
                &app,
                runtime.clone(),
                &conversation_id,
                mode,
                text,
                attachments,
                cancel.clone(),
            )
            .await;

        let cancel_requested = runtime.finish_turn(&conversation_id, &cancel).await;
        let (ok, message) = match &result {
            Ok(()) if cancel_requested => (false, "回合已取消。".to_string()),
            Ok(()) => (true, "回合已完成。".to_string()),
            Err(error) => (false, error.message.clone()),
        };
        let _ = app.emit(
            "agent://turn-finished",
            json!({
                "conversationId": conversation_id,
                "ok": ok,
                "message": message,
            }),
        );
        emit_conversations_changed(&app);
        self.schedule_idle_cleanup(conversation_id, runtime).await;
        result
    }

    async fn run_turn_inner(
        self: &Arc<Self>,
        app: &AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
        mode: AgentTurnMode,
        text: String,
        attachments: Vec<ChatImageAttachment>,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), AgentError> {
        let config = runtime.store.get_agent_backend_config()?;
        let session = self
            .session(app.clone(), runtime.clone(), conversation_id, &config)
            .await?;
        let mut state = TurnRunState::default();
        let result = self
            .run_session_turn(
                app,
                runtime.clone(),
                conversation_id,
                mode,
                text,
                attachments,
                cancel,
                &session,
                &mut state,
            )
            .await;

        if let Some(turn_id) = state.turn_id.as_deref() {
            self.finish_session_turn(&runtime.store, conversation_id, &session, turn_id)
                .await;
        }
        if let Err(error) = &result {
            if (state.started && !state.terminal) || is_transport_failure(error) {
                let invalidated_current = self
                    .invalidate_session(&runtime.store, conversation_id, &session)
                    .await;
                if is_transport_failure(error) && invalidated_current {
                    *self.status.write().await = status_from_error(error.clone());
                }
            }
        }
        result
    }

    async fn run_session_turn(
        &self,
        app: &AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
        mode: AgentTurnMode,
        text: String,
        attachments: Vec<ChatImageAttachment>,
        cancel: Arc<AtomicBool>,
        session: &Arc<CodexSession>,
        state: &mut TurnRunState,
    ) -> Result<(), AgentError> {
        *session.turn_mode.lock().unwrap() = mode;
        *session.turn_cancel.lock().unwrap() = cancel.clone();
        *session.last_used.lock().await = Instant::now();

        if !attachments.is_empty() && !session.image_input_supported {
            return Err(AgentError::new(
                "image_input_unsupported_by_model",
                "当前本地 Codex 模型不支持图片输入。",
            ));
        }

        let mut input = Vec::new();
        if !text.trim().is_empty() {
            input.push(json!({"type": "text", "text": text}));
        }
        input.extend(
            attachments
                .iter()
                .filter(|attachment| attachment.available)
                .map(|attachment| {
                    json!({
                        "type": "localImage",
                        "path": attachment.path,
                        "detail": "auto",
                    })
                }),
        );
        let mut params = json!({
            "threadId": session.thread_id,
            "input": input,
            "cwd": session.cwd,
            "sandboxPolicy": {"type": "readOnly", "networkAccess": false},
            "approvalPolicy": {
                "granular": {
                    "sandbox_approval": false,
                    "rules": false,
                    "mcp_elicitations": true,
                    "request_permissions": false,
                    "skill_approval": false
                }
            }
        });
        if mode == AgentTurnMode::Plan {
            if !session.plan_supported {
                return Err(AgentError::new(
                    "codex_plan_unsupported",
                    "当前本地 Codex 不支持计划模式。",
                ));
            }
            params["collaborationMode"] = json!({
                "mode": "plan",
                "settings": {
                    "model": session.model,
                    "reasoning_effort": null,
                    "developer_instructions": CODEX_DEVELOPER_INSTRUCTIONS
                }
            });
        } else if mode == AgentTurnMode::ConversationOnly {
            params["additionalContext"] = json!({
                "nanabettercubism-mode": {
                    "kind": "application",
                    "value": "This turn is read-only. Do not call mutating MCP tools."
                }
            });
        }

        let response = session.transport.request("turn/start", params).await?;
        state.started = true;
        let turn_id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::new("codex_protocol_error", "turn/start 缺少 turn.id。"))?
            .to_string();
        *session.active_turn.lock().await = Some(turn_id.clone());
        state.turn_id = Some(turn_id.clone());

        let mut plan_steps = Vec::<PlanStep>::new();
        let mut plan_text = String::new();
        let mut policy_violation: Option<AgentError> = None;
        let mut interrupt_sent = false;
        let completed = loop {
            if cancel.load(Ordering::SeqCst) && !interrupt_sent {
                interrupt_sent = true;
                let _ = session
                    .transport
                    .request(
                        "turn/interrupt",
                        json!({"threadId": session.thread_id, "turnId": turn_id}),
                    )
                    .await;
            }

            let Some(event) = session.poll_event(TURN_EVENT_POLL).await? else {
                continue;
            };

            if event.get("id").is_some() && event.get("method").is_some() {
                if !server_request_matches_turn(&event, &session.thread_id, &turn_id) {
                    session
                        .transport
                        .respond_error(event["id"].clone(), -32001, "Codex 请求所属回合已失效。")
                        .await?;
                    continue;
                }
                if let Err(error) = self
                    .handle_server_request(
                        app,
                        runtime.clone(),
                        conversation_id,
                        mode,
                        &session,
                        &turn_id,
                        &event,
                        &cancel,
                    )
                    .await
                {
                    policy_violation = Some(error);
                    if !interrupt_sent {
                        interrupt_sent = true;
                        let _ = session
                            .transport
                            .request(
                                "turn/interrupt",
                                json!({"threadId": session.thread_id, "turnId": turn_id}),
                            )
                            .await;
                    }
                }
                continue;
            }

            let method = event
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = event.get("params").cloned().unwrap_or(Value::Null);
            if !event_matches_turn(&params, &session.thread_id, &turn_id) {
                continue;
            }
            match method {
                "item/agentMessage/delta" => {
                    if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                        let _ = app.emit(
                            "agent://turn-delta",
                            json!({"conversationId": conversation_id, "text": delta}),
                        );
                    }
                }
                "item/completed" => {
                    let item = &params["item"];
                    match item.get("type").and_then(Value::as_str).unwrap_or_default() {
                        "agentMessage" => {
                            let id = item
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or(&new_id())
                                .to_string();
                            let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                            runtime.store.append_external_message_once(
                                conversation_id,
                                &id,
                                "assistant",
                                text,
                                None,
                                None,
                            )?;
                        }
                        "plan" => {
                            plan_text = item
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                        }
                        "mcpToolCall" => {
                            let server = item
                                .get("server")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if server != MCP_SERVER_NAME {
                                policy_violation = Some(AgentError::new(
                                    "codex_policy_violation",
                                    "Codex 调用了未授权的 MCP 服务。",
                                ));
                            }
                        }
                        forbidden if is_forbidden_item(forbidden) => {
                            policy_violation = Some(AgentError::new(
                                "codex_policy_violation",
                                format!("Codex 尝试使用未授权能力：{forbidden}"),
                            ));
                        }
                        _ => {}
                    }
                }
                "item/started" => {
                    let item_type = params
                        .pointer("/item/type")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if is_forbidden_item(item_type) {
                        policy_violation = Some(AgentError::new(
                            "codex_policy_violation",
                            format!("Codex 尝试使用未授权能力：{item_type}"),
                        ));
                    }
                }
                "turn/plan/updated" => {
                    plan_steps = params
                        .get("plan")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|step| {
                            let title = step.get("step")?.as_str()?.trim();
                            if title.is_empty() {
                                return None;
                            }
                            Some(PlanStep {
                                id: new_id(),
                                title: title.to_string(),
                                status: match step.get("status").and_then(Value::as_str) {
                                    Some("inProgress") => "in_progress",
                                    Some("completed") => "completed",
                                    _ => "pending",
                                }
                                .into(),
                            })
                        })
                        .collect();
                    let plan = runtime
                        .store
                        .upsert_plan(conversation_id, plan_steps.clone())?;
                    let _ = app.emit(
                        "agent://plan",
                        json!({"conversationId": conversation_id, "plan": plan}),
                    );
                }
                "turn/completed" => break params,
                "error" => {
                    let message = params
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("本地 Codex 回合失败。");
                    policy_violation = Some(AgentError::new("codex_turn_failed", message));
                }
                "transport/closed" => {
                    return Err(AgentError::new(
                        "codex_transport_closed",
                        session.transport.diagnostic_message(),
                    ))
                }
                _ => {}
            }
        };
        state.terminal = true;

        if let Some(error) = policy_violation {
            return Err(error);
        }
        let status = completed
            .pointer("/turn/status")
            .and_then(Value::as_str)
            .unwrap_or("failed");
        match status {
            "completed" => {}
            "interrupted" => {
                return Err(AgentError::new(
                    "codex_turn_cancelled",
                    "本地 Codex 回合已取消。",
                ))
            }
            _ => {
                let message = completed
                    .pointer("/turn/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("本地 Codex 回合失败。");
                return Err(AgentError::new("codex_turn_failed", message));
            }
        }

        if mode == AgentTurnMode::Plan {
            self.publish_plan_approval(
                app,
                &runtime.store,
                conversation_id,
                plan_steps,
                plan_text,
            )?;
        }
        Ok(())
    }

    async fn handle_server_request(
        &self,
        app: &AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
        mode: AgentTurnMode,
        session: &Arc<CodexSession>,
        turn_id: &str,
        request: &Value,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(), AgentError> {
        let request_id = request["id"].clone();
        let method = request["method"].as_str().unwrap_or_default();
        if method != "item/tool/requestUserInput" {
            session
                .transport
                .respond_error(
                    request_id,
                    -32000,
                    "NanaBetterCubism 拒绝未授权的 Codex 权限或工具请求。",
                )
                .await?;
            return Err(AgentError::new(
                "codex_policy_violation",
                format!("Codex 发起了未授权请求：{method}"),
            ));
        }

        let questions = request
            .pointer("/params/questions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if questions.is_empty() {
            session
                .transport
                .respond(request_id, json!({"answers": {}}))
                .await?;
            return Ok(());
        }

        if mode == AgentTurnMode::AutoApprove {
            if let Some(answers) = automatic_approval_answers(&questions) {
                session
                    .transport
                    .respond(request_id, json!({"answers": answers}))
                    .await?;
                return Ok(());
            }
        }

        let question_text = questions
            .iter()
            .filter_map(|question| question.get("question").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n\n");
        let options = questions
            .first()
            .and_then(|question| question.get("options"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("label").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let action_id = new_id();
        let pending = PendingQuestion {
            action_id: action_id.clone(),
            conversation_id: conversation_id.into(),
            question: question_text,
            options,
        };
        runtime
            .store
            .set_pending_question(&pending, "codex-request-user-input")?;
        let (answer_tx, answer_rx) = oneshot::channel();
        self.pending_answers.lock().await.insert(
            action_id.clone(),
            PendingCodexAnswer {
                sender: answer_tx,
                conversation_id: conversation_id.to_string(),
                generation: session.generation,
                turn_id: turn_id.to_string(),
            },
        );
        let _ = app.emit(
            "agent://user-action",
            json!({
                "conversationId": conversation_id,
                "action": crate::agent::PendingUserAction::from(pending),
            }),
        );

        let wait_result = wait_for_answer(
            answer_rx,
            cancel,
            session.transport.closed_receiver(),
            TURN_EVENT_POLL,
        )
        .await;
        self.pending_answers.lock().await.remove(&action_id);
        let _ = runtime.store.take_pending_question(&action_id)?;

        let answer = match wait_result {
            PendingAnswerResult::Answer(answer) => answer,
            PendingAnswerResult::Cancelled => {
                session
                    .transport
                    .respond(request_id, json!({"answers": {}}))
                    .await?;
                return Ok(());
            }
            PendingAnswerResult::TransportClosed => {
                return Err(AgentError::new(
                    "codex_transport_closed",
                    session.transport.diagnostic_message(),
                ));
            }
            PendingAnswerResult::Expired => {
                return Err(AgentError::new(
                    "codex_question_expired",
                    "Codex 提问上下文已失效。",
                ));
            }
        };
        let answers = questions
            .iter()
            .filter_map(|question| question.get("id").and_then(Value::as_str))
            .map(|id| (id.to_string(), json!({"answers": [answer]})))
            .collect::<serde_json::Map<_, _>>();
        session
            .transport
            .respond(request_id, json!({"answers": answers}))
            .await
    }

    fn publish_plan_approval(
        &self,
        app: &AppHandle,
        store: &AgentStore,
        conversation_id: &str,
        mut steps: Vec<PlanStep>,
        plan_text: String,
    ) -> Result<(), AgentError> {
        if steps.is_empty() {
            steps.push(PlanStep {
                id: new_id(),
                title: plan_text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("执行已确认的 Codex 计划")
                    .trim()
                    .to_string(),
                status: "pending".into(),
            });
        }
        let summary = if plan_text.trim().is_empty() {
            steps
                .iter()
                .map(|step| step.title.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            plan_text.trim().to_string()
        };
        let document = PlanDocument {
            title: summary
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Codex 计划")
                .trim_start_matches('#')
                .trim()
                .chars()
                .take(48)
                .collect(),
            summary,
            steps: steps.iter().map(|step| step.title.clone()).collect(),
            diagram: plan_diagram(steps.len()),
            acceptance: vec!["执行后回读受影响对象并验证语义后置条件。".into()],
            assumptions: vec!["仅使用当前 Editor 会话已确认的协议能力。".into()],
            risks: vec!["事务提交或回滚无法确认时必须停止并报告 unknown。".into()],
        }
        .validate()?;
        let action = PlanApprovalAction {
            action_id: new_id(),
            conversation_id: conversation_id.into(),
            title: document.title.clone(),
        };
        store.set_pending_plan_approval(&PendingPlanApproval {
            action: action.clone(),
            plan: document,
        })?;
        let plan = store.upsert_plan(conversation_id, steps)?;
        let _ = app.emit(
            "agent://plan",
            json!({"conversationId": conversation_id, "plan": plan}),
        );
        let _ = app.emit(
            "agent://user-action",
            json!({
                "conversationId": conversation_id,
                "action": crate::agent::PendingUserAction::from(action),
            }),
        );
        Ok(())
    }

    async fn session(
        &self,
        app: AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
        config: &AgentBackendConfigView,
    ) -> Result<Arc<CodexSession>, AgentError> {
        let existing = self.sessions.lock().await.get(conversation_id).cloned();
        if let Some(session) = existing {
            if session.transport.is_healthy() {
                return Ok(session);
            }
            self.invalidate_session(&runtime.store, conversation_id, &session)
                .await;
        }
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let session = Arc::new(
            CodexSession::start(
                app,
                runtime,
                conversation_id,
                config,
                &self.data_dir,
                generation,
            )
            .await?,
        );
        *self.status.write().await = CodexStatus {
            state: CodexStatusState::Ready,
            version: Some(session.version.clone()),
            model: Some(session.model.clone()),
            image_input_supported: Some(session.image_input_supported),
            plan_supported: session.plan_supported,
            message: format!("本地 Codex {} 已就绪。", session.version),
        };
        self.sessions
            .lock()
            .await
            .insert(conversation_id.to_string(), session.clone());
        Ok(session)
    }

    async fn finish_session_turn(
        &self,
        store: &AgentStore,
        conversation_id: &str,
        session: &Arc<CodexSession>,
        turn_id: &str,
    ) {
        let mut active_turn = session.active_turn.lock().await;
        if active_turn.as_deref() == Some(turn_id) {
            *active_turn = None;
        }
        drop(active_turn);
        *session.last_used.lock().await = Instant::now();
        self.clear_pending_answers(
            store,
            conversation_id,
            Some(session.generation),
            Some(turn_id),
        )
        .await;
    }

    async fn clear_pending_answers(
        &self,
        store: &AgentStore,
        conversation_id: &str,
        generation: Option<u64>,
        turn_id: Option<&str>,
    ) {
        let mut action_ids = Vec::new();
        let mut pending = self.pending_answers.lock().await;
        pending.retain(|action_id, answer| {
            let keep = answer.conversation_id != conversation_id
                || generation.is_some_and(|value| answer.generation != value)
                || turn_id.is_some_and(|value| answer.turn_id != value);
            if !keep {
                action_ids.push(action_id.clone());
            }
            keep
        });
        drop(pending);
        for action_id in action_ids {
            let _ = store.take_pending_question(&action_id);
        }
    }

    async fn invalidate_session(
        &self,
        store: &AgentStore,
        conversation_id: &str,
        session: &Arc<CodexSession>,
    ) -> bool {
        session.transport.shutdown().await;
        *session.active_turn.lock().await = None;
        self.clear_pending_answers(store, conversation_id, Some(session.generation), None)
            .await;
        let mut sessions = self.sessions.lock().await;
        let removed = if sessions
            .get(conversation_id)
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            sessions.remove(conversation_id);
            true
        } else {
            false
        };
        drop(sessions);
        removed
    }

    async fn schedule_idle_cleanup(
        self: &Arc<Self>,
        conversation_id: String,
        runtime: Arc<AgentRuntime>,
    ) {
        let session = self.sessions.lock().await.get(&conversation_id).cloned();
        let Some(session) = session else {
            return;
        };
        let manager = Arc::downgrade(self);
        tokio::spawn(async move {
            sleep(SESSION_IDLE_TIMEOUT).await;
            let Some(manager) = manager.upgrade() else {
                return;
            };
            if session.active_turn.lock().await.is_some()
                || session.last_used.lock().await.elapsed() < SESSION_IDLE_TIMEOUT
            {
                return;
            }
            let is_current = manager
                .sessions
                .lock()
                .await
                .get(&conversation_id)
                .is_some_and(|current| Arc::ptr_eq(current, &session));
            if is_current {
                manager
                    .invalidate_session(&runtime.store, &conversation_id, &session)
                    .await;
            }
        });
    }

    pub async fn archive_conversation(
        &self,
        app: AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
    ) -> Result<bool, AgentError> {
        let Some(thread_id) = runtime.store.codex_thread_id(conversation_id)? else {
            let session = self.sessions.lock().await.get(conversation_id).cloned();
            if let Some(session) = session {
                self.invalidate_session(&runtime.store, conversation_id, &session)
                    .await;
            }
            self.clear_pending_answers(&runtime.store, conversation_id, None, None)
                .await;
            let _ = runtime.store.clear_pending_user_action(conversation_id);
            return Ok(false);
        };
        let config = runtime.store.get_agent_backend_config()?;
        let session = self
            .session(app, runtime.clone(), conversation_id, &config)
            .await?;
        let result = session
            .transport
            .request("thread/archive", json!({"threadId": thread_id}))
            .await;
        self.invalidate_session(&runtime.store, conversation_id, &session)
            .await;
        self.clear_pending_answers(&runtime.store, conversation_id, None, None)
            .await;
        let _ = runtime.store.clear_pending_user_action(conversation_id);
        result?;
        Ok(true)
    }
}

struct CodexSession {
    transport: Arc<JsonRpcTransport>,
    events: Mutex<mpsc::UnboundedReceiver<Value>>,
    _mcp: Option<InternalMcpServer>,
    turn_mode: Arc<StdMutex<AgentTurnMode>>,
    turn_cancel: Arc<StdMutex<Arc<AtomicBool>>>,
    generation: u64,
    thread_id: String,
    model: String,
    version: String,
    image_input_supported: bool,
    plan_supported: bool,
    cwd: String,
    active_turn: Mutex<Option<String>>,
    last_used: Mutex<Instant>,
}

impl CodexSession {
    async fn poll_event(&self, poll_interval: Duration) -> Result<Option<Value>, AgentError> {
        let event = {
            let mut events = self.events.lock().await;
            timeout(poll_interval, events.recv()).await
        };
        match event {
            Err(_) => Ok(None),
            Ok(Some(event)) => Ok(Some(event)),
            Ok(None) => Err(AgentError::new(
                "codex_transport_closed",
                self.transport.diagnostic_message(),
            )),
        }
    }

    #[cfg(test)]
    fn mock(generation: u64) -> (Arc<Self>, mpsc::UnboundedReceiver<Value>) {
        let (transport, events, outbound) = JsonRpcTransport::mock();
        (
            Arc::new(Self {
                transport,
                events: Mutex::new(events),
                _mcp: None,
                turn_mode: Arc::new(StdMutex::new(AgentTurnMode::Default)),
                turn_cancel: Arc::new(StdMutex::new(Arc::new(AtomicBool::new(false)))),
                generation,
                thread_id: format!("thread-{generation}"),
                model: "mock-model".into(),
                version: "mock-version".into(),
                image_input_supported: true,
                plan_supported: true,
                cwd: "mock-cwd".into(),
                active_turn: Mutex::new(None),
                last_used: Mutex::new(Instant::now()),
            }),
            outbound,
        )
    }

    async fn start(
        app: AppHandle,
        runtime: Arc<AgentRuntime>,
        conversation_id: &str,
        config: &AgentBackendConfigView,
        data_dir: &Path,
        generation: u64,
    ) -> Result<Self, AgentError> {
        let executable = resolve_codex_executable(config).await?;
        let version = codex_version(&executable).await?;
        let mcp = start_internal_server(app, runtime.clone(), conversation_id.to_string()).await?;
        let (transport, events) = JsonRpcTransport::spawn(&executable, &mcp.token).await?;
        initialize(&transport).await?;
        ensure_account(&transport).await?;
        let (model, image_input_supported) = read_default_model(&transport).await?;
        let plan_supported = transport
            .request("collaborationMode/list", json!({}))
            .await
            .ok()
            .and_then(|value| value.get("data").and_then(Value::as_array).cloned())
            .is_some_and(|modes| {
                modes.iter().any(|mode| {
                    mode.pointer("/mode").and_then(Value::as_str) == Some("plan")
                        || mode.pointer("/name").and_then(Value::as_str) == Some("plan")
                })
            });

        let cwd_path = data_dir.join("codex-runtime").join(conversation_id);
        std::fs::create_dir_all(&cwd_path)
            .map_err(|error| AgentError::new("codex_runtime_dir", error.to_string()))?;
        let cwd = cwd_path
            .canonicalize()
            .unwrap_or(cwd_path)
            .to_string_lossy()
            .to_string();
        let mcp_config = json!({
            MCP_SERVER_NAME: {
                "url": mcp.url,
                "bearer_token_env_var": MCP_TOKEN_ENV,
                "required": true,
                "enabled": true,
                "enabled_tools": mcp.tool_names,
                "default_tools_approval_mode": "writes",
                "startup_timeout_sec": 10,
                "tool_timeout_sec": 180
            }
        });
        let thread_config = json!({
            "mcp_servers": mcp_config,
            "agents": {"enabled": false},
            "memories": {"use_memories": false, "generate_memories": false},
            "apps": {"_default": {"enabled": false}},
            "allow_login_shell": false
        });
        let existing = runtime.store.codex_thread_id(conversation_id)?;
        let (method, mut params) = if let Some(thread_id) = existing {
            (
                "thread/resume",
                json!({
                    "threadId": thread_id,
                    "cwd": cwd,
                    "model": model,
                    "sandbox": "read-only",
                    "runtimeWorkspaceRoots": [],
                    "developerInstructions": CODEX_DEVELOPER_INSTRUCTIONS,
                    "approvalPolicy": "on-request",
                    "config": thread_config,
                }),
            )
        } else {
            (
                "thread/start",
                json!({
                    "cwd": cwd,
                    "model": model,
                    "sandbox": "read-only",
                    "runtimeWorkspaceRoots": [],
                    "developerInstructions": CODEX_DEVELOPER_INSTRUCTIONS,
                    "approvalPolicy": "on-request",
                    "ephemeral": false,
                    "serviceName": "nanabettercubism",
                    "config": thread_config,
                }),
            )
        };
        params["multiAgentMode"] = json!("explicitRequestOnly");
        let response = transport.request(method, params).await?;
        let thread_id = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::new("codex_protocol_error", "Codex thread 响应缺少 ID。"))?
            .to_string();
        if runtime.store.codex_thread_id(conversation_id)?.is_none() {
            runtime
                .store
                .set_codex_thread_id(conversation_id, &thread_id)?;
        }

        verify_mcp_inventory(&transport, &thread_id, &mcp.tool_names).await?;
        let turn_mode = mcp.turn_mode.clone();
        let turn_cancel = mcp.turn_cancel.clone();
        Ok(Self {
            transport,
            events: Mutex::new(events),
            _mcp: Some(mcp),
            turn_mode,
            turn_cancel,
            generation,
            thread_id,
            model,
            version,
            image_input_supported,
            plan_supported,
            cwd,
            active_turn: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
        })
    }
}

struct JsonRpcTransport {
    writer: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, AgentError>>>>>,
    next_id: AtomicU64,
    diagnostics: Arc<StdMutex<VecDeque<String>>>,
    events: mpsc::UnboundedSender<Value>,
    closed: watch::Sender<bool>,
    healthy: AtomicBool,
    #[cfg(test)]
    mock_writer: Option<mpsc::UnboundedSender<Value>>,
}

impl JsonRpcTransport {
    async fn spawn(
        executable: &OsString,
        mcp_token: &str,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Value>), AgentError> {
        let mut command = Command::new(executable);
        command
            .arg("app-server")
            .arg("--stdio")
            .arg("--disable")
            .arg("shell_tool")
            .arg("--disable")
            .arg("browser_use")
            .arg("--disable")
            .arg("computer_use")
            .arg("--disable")
            .arg("image_generation")
            .arg("--disable")
            .arg("apps")
            .arg("--disable")
            .arg("plugins")
            .arg("--disable")
            .arg("multi_agent")
            .arg("--disable")
            .arg("in_app_browser")
            .arg("-c")
            .arg("mcp_servers={}")
            .arg("-c")
            .arg("agents.enabled=false")
            .arg("-c")
            .arg("memories.use_memories=false")
            .arg("-c")
            .arg("memories.generate_memories=false")
            .env(MCP_TOKEN_ENV, mcp_token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .windows_hide();
        let mut child = command
            .spawn()
            .map_err(|error| AgentError::new("codex_unavailable", error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::new("codex_spawn_failed", "无法打开 Codex stdin。"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::new("codex_spawn_failed", "无法打开 Codex stdout。"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::new("codex_spawn_failed", "无法打开 Codex stderr。"))?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics = Arc::new(StdMutex::new(VecDeque::new()));
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (closed, _) = watch::channel(false);
        let transport = Arc::new(Self {
            writer: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            pending,
            next_id: AtomicU64::new(1),
            diagnostics: diagnostics.clone(),
            events: events_tx,
            closed,
            healthy: AtomicBool::new(true),
            #[cfg(test)]
            mock_writer: None,
        });

        let reader_transport = transport.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                reader_transport.dispatch_incoming(message).await;
            }
            reader_transport.close().await;
        });

        let diagnostics_writer = diagnostics.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut entries = diagnostics_writer.lock().unwrap();
                if entries.len() == MAX_DIAGNOSTICS {
                    entries.pop_front();
                }
                entries.push_back(line);
            }
        });

        Ok((transport, events_rx))
    }

    #[cfg(test)]
    fn mock() -> (
        Arc<Self>,
        mpsc::UnboundedReceiver<Value>,
        mpsc::UnboundedReceiver<Value>,
    ) {
        let (events, events_rx) = mpsc::unbounded_channel();
        let (mock_writer, mock_rx) = mpsc::unbounded_channel();
        let (closed, _) = watch::channel(false);
        (
            Arc::new(Self {
                writer: Mutex::new(None),
                child: Mutex::new(None),
                pending: Arc::new(Mutex::new(HashMap::new())),
                next_id: AtomicU64::new(1),
                diagnostics: Arc::new(StdMutex::new(VecDeque::new())),
                events,
                closed,
                healthy: AtomicBool::new(true),
                mock_writer: Some(mock_writer),
            }),
            events_rx,
            mock_rx,
        )
    }

    async fn dispatch_incoming(&self, message: Value) {
        if !self.is_healthy() {
            return;
        }
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            if message.get("method").is_none() {
                if let Some(sender) = self.pending.lock().await.remove(&id) {
                    let response = if let Some(error) = message.get("error") {
                        Err(AgentError::new(
                            "codex_rpc_error",
                            error
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Codex 请求失败。"),
                        ))
                    } else {
                        Ok(message.get("result").cloned().unwrap_or(Value::Null))
                    };
                    let _ = sender.send(response);
                }
                return;
            }
        }
        let _ = self.events.send(message);
    }

    fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    fn closed_receiver(&self) -> watch::Receiver<bool> {
        self.closed.subscribe()
    }

    fn invalidate(&self) {
        if self.healthy.swap(false, Ordering::SeqCst) {
            self.closed.send_replace(true);
            let _ = self
                .events
                .send(json!({"method": "transport/closed", "params": {}}));
        }
    }

    async fn close(&self) {
        self.invalidate();
        let mut pending = self.pending.lock().await;
        for (_, sender) in pending.drain() {
            let _ = sender.send(Err(AgentError::new(
                "codex_transport_closed",
                "本地 Codex 进程已退出。",
            )));
        }
    }

    async fn shutdown(&self) {
        self.close().await;
        if let Some(child) = self.child.lock().await.as_mut() {
            let _ = child.start_kill();
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        if !self.is_healthy() {
            return Err(AgentError::new(
                "codex_transport_closed",
                self.diagnostic_message(),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        if let Err(error) = self
            .write(json!({"method": method, "id": id, "params": params}))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        match timeout(REQUEST_TIMEOUT, receiver).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => Err(AgentError::new(
                "codex_transport_closed",
                self.diagnostic_message(),
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(AgentError::new(
                    "codex_request_timeout",
                    format!("Codex 请求超时：{method}"),
                ))
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), AgentError> {
        self.write(json!({"method": method, "params": params}))
            .await
    }

    async fn respond(&self, id: Value, result: Value) -> Result<(), AgentError> {
        self.write(json!({"id": id, "result": result})).await
    }

    async fn respond_error(&self, id: Value, code: i64, message: &str) -> Result<(), AgentError> {
        self.write(json!({"id": id, "error": {"code": code, "message": message}}))
            .await
    }

    async fn write(&self, message: Value) -> Result<(), AgentError> {
        if !self.is_healthy() {
            return Err(AgentError::new(
                "codex_transport_closed",
                self.diagnostic_message(),
            ));
        }
        #[cfg(test)]
        if let Some(writer) = &self.mock_writer {
            return writer.send(message).map_err(|_| {
                AgentError::new("codex_transport_closed", "Mock Codex transport 已关闭。")
            });
        }
        let mut bytes = serde_json::to_vec(&message)?;
        bytes.push(b'\n');
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| AgentError::new("codex_transport_closed", self.diagnostic_message()))?;
        writer
            .write_all(&bytes)
            .await
            .map_err(|error| AgentError::new("codex_transport_write", error.to_string()))?;
        writer
            .flush()
            .await
            .map_err(|error| AgentError::new("codex_transport_write", error.to_string()))
    }

    fn diagnostic_message(&self) -> String {
        self.diagnostics
            .lock()
            .unwrap()
            .back()
            .cloned()
            .unwrap_or_else(|| "本地 Codex 进程已退出。".into())
    }
}

impl Drop for JsonRpcTransport {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.try_lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.start_kill();
            }
        }
    }
}

async fn initialize(transport: &JsonRpcTransport) -> Result<(), AgentError> {
    transport
        .request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "nanabettercubism",
                    "title": "NanaBetterCubism",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true,
                    "mcpServerOpenaiFormElicitation": false
                }
            }),
        )
        .await?;
    transport.notify("initialized", json!({})).await
}

async fn ensure_account(transport: &JsonRpcTransport) -> Result<(), AgentError> {
    let account = transport
        .request("account/read", json!({"refreshToken": false}))
        .await?;
    let requires_auth = account
        .get("requiresOpenaiAuth")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if requires_auth && account.get("account").is_none_or(Value::is_null) {
        return Err(AgentError::new(
            "codex_auth_required",
            "本地 Codex 尚未登录，请先在 Codex CLI 完成登录。",
        ));
    }
    Ok(())
}

async fn read_default_model(transport: &JsonRpcTransport) -> Result<(String, bool), AgentError> {
    let models = transport
        .request("model/list", json!({"limit": 100, "includeHidden": false}))
        .await?;
    let data = models
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AgentError::new("codex_protocol_error", "model/list 缺少 data。"))?;
    let model = data
        .iter()
        .find(|item| item.get("isDefault").and_then(Value::as_bool) == Some(true))
        .or_else(|| data.first())
        .ok_or_else(|| AgentError::new("codex_no_model", "本地 Codex 没有可用模型。"))?;
    let id = model
        .get("id")
        .or_else(|| model.get("model"))
        .and_then(Value::as_str)
        .ok_or_else(|| AgentError::new("codex_protocol_error", "Codex 模型缺少 ID。"))?;
    let image = model
        .get("inputModalities")
        .and_then(Value::as_array)
        .map(|values| values.iter().any(|value| value.as_str() == Some("image")))
        .unwrap_or(true);
    Ok((id.to_string(), image))
}

async fn verify_mcp_inventory(
    transport: &JsonRpcTransport,
    thread_id: &str,
    expected_tools: &[String],
) -> Result<(), AgentError> {
    let status = transport
        .request(
            "mcpServerStatus/list",
            json!({
                "threadId": thread_id,
                "detail": "toolsAndAuthOnly",
                "limit": 100
            }),
        )
        .await?;
    let servers = status
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AgentError::new("codex_protocol_error", "MCP 状态缺少 data。"))?;
    if servers.len() != 1 || servers[0].get("name").and_then(Value::as_str) != Some(MCP_SERVER_NAME)
    {
        return Err(AgentError::new(
            "codex_incompatible",
            "Codex 有效 MCP 配置包含未授权服务，已拒绝启动。",
        ));
    }
    let actual = servers[0]
        .get("tools")
        .and_then(Value::as_object)
        .map(|tools| tools.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let expected = expected_tools.iter().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(AgentError::new(
            "codex_incompatible",
            "Codex MCP 工具清单与 NanaBetterCubism 允许清单不一致。",
        ));
    }
    Ok(())
}

async fn probe_codex(config: &AgentBackendConfigView) -> Result<CodexStatus, AgentError> {
    let executable = resolve_codex_executable(config).await?;
    let version = codex_version(&executable).await?;
    let (transport, _events) = JsonRpcTransport::spawn(&executable, "probe-only").await?;
    initialize(&transport).await?;
    ensure_account(&transport).await?;
    let (model, image_input_supported) = read_default_model(&transport).await?;
    let plan_supported = transport
        .request("collaborationMode/list", json!({}))
        .await
        .is_ok();
    Ok(CodexStatus {
        state: CodexStatusState::Ready,
        version: Some(version.clone()),
        model: Some(model),
        image_input_supported: Some(image_input_supported),
        plan_supported,
        message: format!("本地 Codex {version} 已登录并可用。"),
    })
}

async fn resolve_codex_executable(config: &AgentBackendConfigView) -> Result<OsString, AgentError> {
    if let Some(path) = config
        .codex_executable
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(AgentError::new(
                "codex_unavailable",
                "指定的 Codex 可执行文件不存在。",
            ));
        }
        return Ok(path.into_os_string());
    }

    if Command::new("codex")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
    {
        return Ok(OsString::from("codex"));
    }

    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
                .join("codex.exe"),
        );
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        candidates.push(
            PathBuf::from(profile)
                .join(".local")
                .join("bin")
                .join("codex.exe"),
        );
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(PathBuf::into_os_string)
        .ok_or_else(|| {
            AgentError::new(
                "codex_unavailable",
                "未找到本地 Codex CLI；请安装 Codex 或在设置中指定路径。",
            )
        })
}

async fn codex_version(executable: &OsString) -> Result<String, AgentError> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .await
        .map_err(|error| AgentError::new("codex_unavailable", error.to_string()))?;
    if !output.status.success() {
        return Err(AgentError::new(
            "codex_unavailable",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn status_from_error(error: AgentError) -> CodexStatus {
    let state = match error.code.as_str() {
        "codex_unavailable" => CodexStatusState::Unavailable,
        "codex_auth_required" => CodexStatusState::AuthRequired,
        "codex_incompatible" => CodexStatusState::Incompatible,
        _ => CodexStatusState::Failed,
    };
    CodexStatus {
        state,
        message: error.message,
        ..CodexStatus::default()
    }
}

fn is_forbidden_item(item_type: &str) -> bool {
    matches!(
        item_type,
        "commandExecution"
            | "fileChange"
            | "webSearch"
            | "imageGeneration"
            | "dynamicToolCall"
            | "collabAgentToolCall"
            | "subAgentActivity"
    )
}

fn server_request_matches_turn(request: &Value, thread_id: &str, turn_id: &str) -> bool {
    request.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
        && request.pointer("/params/turnId").and_then(Value::as_str) == Some(turn_id)
}

fn event_matches_turn(params: &Value, thread_id: &str, turn_id: &str) -> bool {
    let thread_matches = params
        .get("threadId")
        .and_then(Value::as_str)
        .is_none_or(|id| id == thread_id);
    let event_turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .or_else(|| params.pointer("/turn/id").and_then(Value::as_str));
    thread_matches && event_turn_id.is_none_or(|id| id == turn_id)
}

async fn wait_for_answer(
    mut answer_rx: oneshot::Receiver<String>,
    cancel: &AtomicBool,
    mut transport_closed: watch::Receiver<bool>,
    poll_interval: Duration,
) -> PendingAnswerResult {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return PendingAnswerResult::Cancelled;
        }
        if *transport_closed.borrow() {
            return PendingAnswerResult::TransportClosed;
        }

        tokio::select! {
            biased;
            changed = transport_closed.changed() => {
                if changed.is_err() || *transport_closed.borrow() {
                    return PendingAnswerResult::TransportClosed;
                }
            }
            answer = &mut answer_rx => {
                return match answer {
                    Ok(answer) => PendingAnswerResult::Answer(answer),
                    Err(_) => PendingAnswerResult::Expired,
                };
            }
            _ = sleep(poll_interval) => {}
        }
    }
}

fn is_transport_failure(error: &AgentError) -> bool {
    matches!(
        error.code.as_str(),
        "codex_transport_closed" | "codex_transport_write" | "codex_request_timeout"
    )
}

fn automatic_approval_answers(questions: &[Value]) -> Option<serde_json::Map<String, Value>> {
    let mut answers = serde_json::Map::new();
    for question in questions {
        let id = question.get("id")?.as_str()?;
        let label = question
            .get("options")?
            .as_array()?
            .iter()
            .filter_map(|option| option.get("label").and_then(Value::as_str))
            .find(|label| {
                let normalized = label.to_ascii_lowercase();
                normalized.contains("accept")
                    || normalized.contains("approve")
                    || normalized.contains("allow")
                    || label.contains('允')
                    || label.contains('批')
            })?;
        answers.insert(id.to_string(), json!({"answers": [label]}));
    }
    Some(answers)
}

fn plan_diagram(step_count: usize) -> String {
    let mut lines = vec!["flowchart TD".to_string()];
    for index in 0..step_count.max(1) {
        lines.push(format!("S{}[\"步骤 {}\"]", index + 1, index + 1));
        if index > 0 {
            lines.push(format!("S{} --> S{}", index, index + 1));
        }
    }
    lines.join("\n")
}

trait CommandWindowsHide {
    fn windows_hide(&mut self) -> &mut Self;
}

impl CommandWindowsHide for Command {
    fn windows_hide(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.as_std_mut().creation_flags(0x08000000);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (AgentStore, String, PathBuf) {
        let dir = std::env::temp_dir().join(format!("nbc-codex-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = AgentStore::default();
        store.open(dir.join("agent.db")).unwrap();
        let conversation = store
            .create_conversation(Some("Codex transport test".into()), None)
            .unwrap();
        (store, conversation.id, dir)
    }

    #[test]
    fn auto_approval_accepts_only_explicit_allow_options() {
        let questions = vec![json!({
            "id": "approval",
            "question": "执行工具？",
            "options": [
                {"label": "Accept", "description": "执行"},
                {"label": "Decline", "description": "拒绝"}
            ]
        })];
        assert_eq!(
            automatic_approval_answers(&questions).unwrap()["approval"]["answers"][0],
            "Accept"
        );
        assert!(automatic_approval_answers(&[json!({
            "id": "choice",
            "question": "选择对象",
            "options": [{"label": "PartA", "description": "对象"}]
        })])
        .is_none());
    }

    #[test]
    fn forbidden_item_inventory_fails_closed() {
        for item in [
            "commandExecution",
            "fileChange",
            "webSearch",
            "imageGeneration",
            "dynamicToolCall",
            "collabAgentToolCall",
        ] {
            assert!(is_forbidden_item(item));
        }
        assert!(!is_forbidden_item("mcpToolCall"));
        assert!(!is_forbidden_item("agentMessage"));
    }

    #[test]
    fn stale_turn_requests_and_notifications_are_rejected() {
        let current_request = json!({
            "id": 1,
            "method": "item/tool/requestUserInput",
            "params": {"threadId": "thread", "turnId": "turn-2", "questions": []}
        });
        assert!(server_request_matches_turn(
            &current_request,
            "thread",
            "turn-2"
        ));
        assert!(!server_request_matches_turn(
            &json!({
                "id": 2,
                "method": "item/tool/requestUserInput",
                "params": {"threadId": "thread", "turnId": "turn-1", "questions": []}
            }),
            "thread",
            "turn-2"
        ));
        assert!(!event_matches_turn(
            &json!({
                "threadId": "thread",
                "turn": {"id": "turn-1", "status": "completed"}
            }),
            "thread",
            "turn-2"
        ));
    }

    #[test]
    fn status_error_mapping_is_typed() {
        assert_eq!(
            status_from_error(AgentError::new("codex_auth_required", "login")).state,
            CodexStatusState::AuthRequired
        );
        assert_eq!(
            status_from_error(AgentError::new("codex_incompatible", "tools")).state,
            CodexStatusState::Incompatible
        );
    }

    #[tokio::test]
    async fn request_user_input_answer_survives_multiple_poll_timeouts() {
        let (answer_tx, answer_rx) = oneshot::channel();
        let (_closed_tx, closed_rx) = watch::channel(false);
        let cancel = Arc::new(AtomicBool::new(false));
        let waiter_cancel = cancel.clone();
        let waiter = tokio::spawn(async move {
            wait_for_answer(answer_rx, &waiter_cancel, closed_rx, TURN_EVENT_POLL).await
        });

        sleep(TURN_EVENT_POLL * 3).await;
        answer_tx.send("继续".into()).unwrap();

        assert_eq!(
            timeout(Duration::from_secs(2), waiter)
                .await
                .unwrap()
                .unwrap(),
            PendingAnswerResult::Answer("继续".into())
        );
    }

    #[tokio::test]
    async fn request_user_input_has_one_terminal_for_cancel_and_transport_close() {
        let (answer_tx, answer_rx) = oneshot::channel::<String>();
        let (closed_tx, closed_rx) = watch::channel(false);
        let cancel = AtomicBool::new(true);
        closed_tx.send_replace(true);

        assert_eq!(
            wait_for_answer(answer_rx, &cancel, closed_rx, TURN_EVENT_POLL).await,
            PendingAnswerResult::Cancelled
        );
        assert!(answer_tx.send("迟到答案".into()).is_err());
    }

    #[tokio::test]
    async fn mock_transport_completes_request_and_turn() {
        let (session, mut outbound) = CodexSession::mock(7);
        let transport = session.transport.clone();
        let request = tokio::spawn(async move {
            transport
                .request("turn/start", json!({"threadId": "thread-7"}))
                .await
        });
        let outbound_request = timeout(Duration::from_secs(2), outbound.recv())
            .await
            .unwrap()
            .unwrap();
        let request_id = outbound_request["id"].clone();
        session
            .transport
            .dispatch_incoming(json!({
                "id": request_id,
                "result": {"turn": {"id": "turn-7"}}
            }))
            .await;
        assert_eq!(request.await.unwrap().unwrap()["turn"]["id"], "turn-7");

        session
            .transport
            .dispatch_incoming(json!({
                "method": "turn/completed",
                "params": {"turn": {"status": "completed"}}
            }))
            .await;
        let event = session
            .poll_event(Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event["method"], "turn/completed");
    }

    #[tokio::test]
    async fn mock_transport_close_expires_waiting_input_and_pending_request() {
        let (session, mut outbound) = CodexSession::mock(11);
        let (answer_tx, answer_rx) = oneshot::channel::<String>();
        let cancel = Arc::new(AtomicBool::new(false));
        let waiter_cancel = cancel.clone();
        let closed = session.transport.closed_receiver();
        let answer_waiter = tokio::spawn(async move {
            wait_for_answer(answer_rx, &waiter_cancel, closed, TURN_EVENT_POLL).await
        });

        let transport = session.transport.clone();
        let request = tokio::spawn(async move { transport.request("turn/start", json!({})).await });
        timeout(Duration::from_secs(2), outbound.recv())
            .await
            .unwrap()
            .unwrap();
        session.transport.close().await;

        assert_eq!(
            answer_waiter.await.unwrap(),
            PendingAnswerResult::TransportClosed
        );
        assert!(answer_tx.send("迟到答案".into()).is_err());
        assert_eq!(
            request.await.unwrap().unwrap_err().code,
            "codex_transport_closed"
        );
        assert_eq!(
            session
                .poll_event(Duration::from_secs(1))
                .await
                .unwrap()
                .unwrap()["method"],
            "transport/closed"
        );
    }

    #[tokio::test]
    async fn fault_cleanup_clears_turn_question_and_allows_retry_session() {
        let (store, conversation_id, dir) = test_store();
        let manager = CodexManager::new(Some(dir.clone()));
        let (failed_session, _outbound) = CodexSession::mock(21);
        *failed_session.active_turn.lock().await = Some("turn-21".into());
        manager
            .sessions
            .lock()
            .await
            .insert(conversation_id.clone(), failed_session.clone());

        let action_id = "action-21".to_string();
        store
            .set_pending_question(
                &PendingQuestion {
                    action_id: action_id.clone(),
                    conversation_id: conversation_id.clone(),
                    question: "继续？".into(),
                    options: vec!["继续".into()],
                },
                "codex-request-user-input",
            )
            .unwrap();
        let (answer_tx, answer_rx) = oneshot::channel();
        manager.pending_answers.lock().await.insert(
            action_id,
            PendingCodexAnswer {
                sender: answer_tx,
                conversation_id: conversation_id.clone(),
                generation: 21,
                turn_id: "turn-21".into(),
            },
        );

        manager
            .invalidate_session(&store, &conversation_id, &failed_session)
            .await;
        assert!(!failed_session.transport.is_healthy());
        assert!(failed_session.active_turn.lock().await.is_none());
        assert!(manager
            .sessions
            .lock()
            .await
            .get(&conversation_id)
            .is_none());
        assert!(manager.pending_answers.lock().await.is_empty());
        assert!(store
            .get_pending_question(&conversation_id)
            .unwrap()
            .is_none());
        assert!(answer_rx.await.is_err());

        let (retry_session, _outbound) = CodexSession::mock(22);
        manager
            .sessions
            .lock()
            .await
            .insert(conversation_id.clone(), retry_session.clone());
        let current = manager
            .sessions
            .lock()
            .await
            .get(&conversation_id)
            .cloned()
            .unwrap();
        assert_eq!(current.generation, 22);
        assert!(current.transport.is_healthy());

        drop(current);
        drop(retry_session);
        drop(failed_session);
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn turn_finalizer_clears_only_its_generation() {
        let store = AgentStore::default();
        let manager = CodexManager::new(None);
        let (session, _outbound) = CodexSession::mock(40);
        *session.active_turn.lock().await = Some("turn-40".into());
        let (old_tx, old_rx) = oneshot::channel();
        let (new_tx, new_rx) = oneshot::channel();
        let mut pending = manager.pending_answers.lock().await;
        pending.insert(
            "old-action".into(),
            PendingCodexAnswer {
                sender: old_tx,
                conversation_id: "conversation".into(),
                generation: 40,
                turn_id: "turn-40".into(),
            },
        );
        pending.insert(
            "new-action".into(),
            PendingCodexAnswer {
                sender: new_tx,
                conversation_id: "conversation".into(),
                generation: 41,
                turn_id: "turn-41".into(),
            },
        );
        drop(pending);

        manager
            .finish_session_turn(&store, "conversation", &session, "turn-40")
            .await;

        assert!(session.active_turn.lock().await.is_none());
        assert!(old_rx.await.is_err());
        assert!(manager
            .answer_question("new-action", "新答案".into())
            .await
            .unwrap());
        assert_eq!(new_rx.await.unwrap(), "新答案");
    }

    #[tokio::test]
    async fn stale_session_event_is_isolated() {
        let (old_session, _old_outbound) = CodexSession::mock(31);
        let (new_session, _new_outbound) = CodexSession::mock(32);
        old_session
            .transport
            .dispatch_incoming(json!({
                "method": "item/agentMessage/delta",
                "params": {"delta": "旧事件"}
            }))
            .await;
        new_session
            .transport
            .dispatch_incoming(json!({
                "method": "turn/completed",
                "params": {"turn": {"status": "completed"}}
            }))
            .await;

        let event = new_session
            .poll_event(Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event["method"], "turn/completed");
    }
}
