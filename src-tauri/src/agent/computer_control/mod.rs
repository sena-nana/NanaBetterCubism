#[cfg(windows)]
mod windows;

use crate::agent::{new_id, AgentError};
use crate::service::{OperationCoordinator, OperationOwnerKind, OperationPermit};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

const GRANT_LIFETIME: Duration = Duration::from_secs(5 * 60);
const MAX_GESTURES: u32 = 30;
const MAX_SETTLE_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedCapability {
    ArtMeshGeometry,
    ArtMeshUvTopology,
    WarpControlPoints,
    AnimationEditing,
    PhysicsEditing,
    SaveExport,
    TextureAtlas,
    PsdOperations,
    GlueCreation,
    ArtPath,
}

impl UnsupportedCapability {
    pub fn label(self) -> &'static str {
        match self {
            Self::ArtMeshGeometry => "ArtMesh 几何编辑",
            Self::ArtMeshUvTopology => "ArtMesh UV 与拓扑编辑",
            Self::WarpControlPoints => "Warp Deformer 控制点编辑",
            Self::AnimationEditing => "动画编辑",
            Self::PhysicsEditing => "物理编辑",
            Self::SaveExport => "保存或导出",
            Self::TextureAtlas => "纹理图集操作",
            Self::PsdOperations => "PSD 操作",
            Self::GlueCreation => "Glue 创建",
            Self::ArtPath => "ArtPath 创建或读取",
        }
    }

    fn available_in_first_release(self) -> bool {
        !matches!(
            self,
            Self::SaveExport | Self::TextureAtlas | Self::PsdOperations
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerActionKind {
    Click,
    DoubleClick,
    Drag,
    Scroll,
    Key,
    TypeText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerOperationStep {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerApproval {
    pub action_id: String,
    pub conversation_id: String,
    pub goal: String,
    pub reason: String,
    pub target_window_title: String,
    pub steps: Vec<ComputerOperationStep>,
    pub allowed_actions: Vec<ComputerActionKind>,
    pub includes_file_dialogs: bool,
    pub impact: String,
    pub cannot_undo: bool,
    pub expires_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerOperationStatus {
    AwaitingApproval,
    Authorized,
    Running,
    Completed,
    NeedsUserVerification,
    Cancelled,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerWindow {
    pub window_id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerFrame {
    pub frame_id: String,
    pub window_id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

pub struct CapturedComputerFrame {
    pub frame: ComputerFrame,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputerAction {
    Click {
        x: i32,
        y: i32,
        #[serde(default)]
        button: MouseButton,
    },
    DoubleClick {
        x: i32,
        y: i32,
        #[serde(default)]
        button: MouseButton,
    },
    Drag {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        #[serde(default = "default_drag_duration")]
        duration_ms: u64,
    },
    Scroll {
        x: i32,
        y: i32,
        delta: i32,
    },
    Key {
        key: String,
        #[serde(default)]
        modifiers: Vec<KeyModifier>,
    },
    TypeText {
        text: String,
    },
}

impl ComputerAction {
    pub fn kind(&self) -> ComputerActionKind {
        match self {
            Self::Click { .. } => ComputerActionKind::Click,
            Self::DoubleClick { .. } => ComputerActionKind::DoubleClick,
            Self::Drag { .. } => ComputerActionKind::Drag,
            Self::Scroll { .. } => ComputerActionKind::Scroll,
            Self::Key { .. } => ComputerActionKind::Key,
            Self::TypeText { .. } => ComputerActionKind::TypeText,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyModifier {
    Ctrl,
    Shift,
    Alt,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerOperationOutcome {
    Completed,
    NeedsUserVerification,
    Partial,
    Failed,
    Unknown,
}

fn default_drag_duration() -> u64 {
    500
}

#[derive(Clone)]
pub struct ComputerControlService {
    inner: Arc<Mutex<ComputerControlState>>,
    coordinator: OperationCoordinator,
}

#[derive(Default)]
struct ComputerControlState {
    windows: HashMap<String, PlatformWindow>,
    pending: HashMap<String, PendingOperation>,
    grant: Option<ComputerGrant>,
    frames: HashMap<String, FrameRecord>,
}

impl Drop for ComputerControlState {
    fn drop(&mut self) {
        if let Some(path) = self.grant.take().and_then(|grant| grant.cache_dir) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

struct PendingOperation {
    approval: ComputerApproval,
    target_window_id: String,
    target: PlatformWindow,
    capability: UnsupportedCapability,
    allowed_actions: BTreeSet<ComputerActionKind>,
    step_ids: BTreeSet<String>,
    document_instance_key: Option<String>,
    expires_at: Instant,
}

struct ComputerGrant {
    id: String,
    conversation_id: String,
    target_window_id: String,
    target: PlatformWindow,
    allowed_actions: BTreeSet<ComputerActionKind>,
    step_ids: BTreeSet<String>,
    includes_file_dialogs: bool,
    document_instance_key: Option<String>,
    expires_at: Instant,
    action_count: u32,
    cache_dir: Option<PathBuf>,
    _permit: OperationPermit,
}

#[derive(Clone)]
struct FrameRecord {
    grant_id: String,
    window_id: String,
    window: PlatformWindow,
    geometry: PlatformGeometry,
    last_input_tick: u32,
}

#[derive(Debug, Clone)]
struct PlatformWindow {
    handle: isize,
    process_id: u32,
    process_started: u64,
    title: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlatformGeometry {
    screen_x: i32,
    screen_y: i32,
    width: u32,
    height: u32,
    dpi: u32,
}

struct PlatformCapture {
    path: String,
    geometry: PlatformGeometry,
    last_input_tick: u32,
}

impl ComputerControlService {
    pub fn new(coordinator: OperationCoordinator) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ComputerControlState::default())),
            coordinator,
        }
    }

    pub fn list_windows(&self, grant_id: Option<&str>) -> Result<Vec<ComputerWindow>, AgentError> {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        let process_filter = match grant_id {
            Some(id) => {
                let grant = require_grant(&state, id)?;
                Some((grant.target.process_id, grant.target.process_started))
            }
            None => None,
        };
        let discovered = enumerate_windows(process_filter)?;
        if grant_id.is_some() && discovered.is_empty() {
            if let Some(id) = grant_id {
                revoke_grant(&mut state, id);
            }
            return Err(AgentError::new(
                "process_changed",
                "已授权的 Cubism 进程已经变化，需重新授权。",
            ));
        }
        state.windows.clear();
        let mut result = Vec::with_capacity(discovered.len());
        for window in discovered {
            let window_id = new_id();
            result.push(ComputerWindow {
                window_id: window_id.clone(),
                title: window.title.clone(),
                width: window.width,
                height: window.height,
            });
            state.windows.insert(window_id, window);
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request_approval(
        &self,
        conversation_id: &str,
        target_window_id: &str,
        capability: UnsupportedCapability,
        goal: String,
        steps: Vec<ComputerOperationStep>,
        allowed_actions: Vec<ComputerActionKind>,
        includes_file_dialogs: bool,
        document_instance_key: Option<String>,
    ) -> Result<ComputerApproval, AgentError> {
        if !capability.available_in_first_release() {
            return Err(AgentError::new(
                "computer_capability_not_validated",
                "该类电脑代理操作尚未通过真实 Editor 验证，当前不可用。",
            ));
        }
        validate_plan(&goal, &steps, &allowed_actions)?;
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        if state.grant.is_some() || !state.pending.is_empty() {
            return Err(AgentError::new(
                "computer_operation_active",
                "已有电脑代理操作正在等待授权或执行。",
            ));
        }
        let target = state
            .windows
            .get(target_window_id)
            .cloned()
            .ok_or_else(|| AgentError::new("stale_window", "目标窗口已失效，请重新列出窗口。"))?;
        if !window_is_current(&target) {
            return Err(AgentError::new("stale_window", "目标窗口已经变化。"));
        }
        let action_id = new_id();
        let expires_at = Instant::now() + GRANT_LIFETIME;
        let approval = ComputerApproval {
            action_id: action_id.clone(),
            conversation_id: conversation_id.to_string(),
            goal,
            reason: format!(
                "Cubism 没有可用于“{}”的官方 API，只能由 Agent 代理操作 Cubism 窗口。",
                capability.label()
            ),
            target_window_title: target.title.clone(),
            steps: steps.clone(),
            allowed_actions: allowed_actions.clone(),
            includes_file_dialogs,
            impact: "Agent 将按上述步骤向这个 Cubism 进程注入鼠标或键盘输入。".into(),
            cannot_undo: true,
            expires_at: (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339(),
        };
        state.pending.insert(
            action_id,
            PendingOperation {
                approval: approval.clone(),
                target_window_id: target_window_id.to_string(),
                target,
                capability,
                allowed_actions: allowed_actions.into_iter().collect(),
                step_ids: steps.into_iter().map(|step| step.id).collect(),
                document_instance_key,
                expires_at,
            },
        );
        Ok(approval)
    }

    pub fn decide(&self, action_id: &str, approved: bool) -> Result<Value, AgentError> {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        if !approved {
            let pending = state
                .pending
                .remove(action_id)
                .ok_or_else(|| AgentError::new("approval_not_found", "电脑代理授权请求已失效。"))?;
            return Ok(json!({
                "approved": false,
                "capability": pending.capability,
            }));
        }
        let pending = state
            .pending
            .get(action_id)
            .ok_or_else(|| AgentError::new("approval_not_found", "电脑代理授权请求已失效。"))?;
        if pending.expires_at <= Instant::now() || !window_is_current(&pending.target) {
            state.pending.remove(action_id);
            return Err(AgentError::new(
                "approval_expired",
                "目标窗口或授权请求已失效，请重新确认。",
            ));
        }
        let grant_id = new_id();
        let permit = self
            .coordinator
            .try_acquire(OperationOwnerKind::ComputerControl, &grant_id)
            .map_err(|_| {
                AgentError::new(
                    "operation_active",
                    "Editor 编辑事务或另一项电脑代理操作正在执行。",
                )
            })?;
        let pending = state.pending.remove(action_id).unwrap();
        state.grant = Some(ComputerGrant {
            id: grant_id.clone(),
            conversation_id: pending.approval.conversation_id,
            target_window_id: pending.target_window_id,
            target: pending.target,
            allowed_actions: pending.allowed_actions,
            step_ids: pending.step_ids,
            includes_file_dialogs: pending.approval.includes_file_dialogs,
            document_instance_key: pending.document_instance_key,
            expires_at: Instant::now() + GRANT_LIFETIME,
            action_count: 0,
            cache_dir: None,
            _permit: permit,
        });
        Ok(json!({
            "approved": true,
            "grantId": grant_id,
            "maxActions": MAX_GESTURES,
        }))
    }

    pub fn pending_approval(&self, action_id: &str) -> Option<ComputerApproval> {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        state
            .pending
            .get(action_id)
            .map(|pending| pending.approval.clone())
    }

    pub fn pending_approval_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Option<ComputerApproval> {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        state
            .pending
            .values()
            .find(|pending| pending.approval.conversation_id == conversation_id)
            .map(|pending| pending.approval.clone())
    }

    pub fn has_active_grant(&self, conversation_id: &str) -> bool {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        state
            .grant
            .as_ref()
            .is_some_and(|grant| grant.conversation_id == conversation_id)
    }

    pub fn capture_frame(
        &self,
        conversation_id: &str,
        grant_id: &str,
        window_id: Option<&str>,
        cache_root: &Path,
        current_document_instance_key: Option<&str>,
    ) -> Result<CapturedComputerFrame, AgentError> {
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        let selection = {
            let grant = require_grant_for_conversation(&state, grant_id, conversation_id)?;
            verify_document(grant, current_document_instance_key)
                .and_then(|_| select_window(&state, grant, window_id))
        };
        let (selected_id, window) = match selection {
            Ok(selected) => selected,
            Err(error) => {
                if matches!(
                    error.code.as_str(),
                    "document_changed" | "window_not_approved"
                ) {
                    revoke_grant(&mut state, grant_id);
                }
                return Err(error);
            }
        };
        let frame_id = new_id();
        let operation_dir = cache_root.join("computer-operation").join(grant_id);
        let capture = match capture_window(&window, &operation_dir, &frame_id) {
            Ok(capture) => capture,
            Err(error) => {
                if error.code == "stale_window" {
                    revoke_grant(&mut state, grant_id);
                }
                return Err(error);
            }
        };
        state.frames.retain(|_, frame| frame.grant_id != grant_id);
        state.frames.insert(
            frame_id.clone(),
            FrameRecord {
                grant_id: grant_id.to_string(),
                window_id: selected_id.clone(),
                window: window.clone(),
                geometry: capture.geometry,
                last_input_tick: capture.last_input_tick,
            },
        );
        if let Some(grant) = state.grant.as_mut() {
            grant.cache_dir = Some(operation_dir);
        }
        Ok(CapturedComputerFrame {
            frame: ComputerFrame {
                frame_id,
                window_id: selected_id,
                title: window.title,
                width: capture.geometry.width,
                height: capture.geometry.height,
            },
            path: capture.path,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn perform_action(
        &self,
        conversation_id: &str,
        grant_id: &str,
        frame_id: &str,
        step_id: &str,
        action: &ComputerAction,
        settle_ms: u64,
        cache_root: &Path,
        current_document_instance_key: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<CapturedComputerFrame, AgentError> {
        if settle_ms > MAX_SETTLE_MS {
            return Err(AgentError::new(
                "invalid_arguments",
                "settleMs 不能超过 2000。",
            ));
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(AgentError::new("cancelled", "已取消。"));
        }
        let mut state = self.inner.lock().unwrap();
        expire_state(&mut state);
        let frame = state
            .frames
            .get(frame_id)
            .cloned()
            .ok_or_else(|| AgentError::new("stale_frame", "截屏已失效，请重新获取。"))?;
        if frame.grant_id != grant_id {
            return Err(AgentError::new("stale_frame", "截屏不属于当前授权。"));
        }
        let validation = (|| {
            let grant = require_grant_for_conversation(&state, grant_id, conversation_id)?;
            verify_document(grant, current_document_instance_key)?;
            if grant.action_count >= MAX_GESTURES {
                return Err(AgentError::new(
                    "computer_action_limit",
                    "本次授权的操作次数已用完。",
                ));
            }
            if !grant.step_ids.contains(step_id) {
                return Err(AgentError::new(
                    "plan_changed",
                    "操作步骤不在已授权计划中。",
                ));
            }
            if !grant.allowed_actions.contains(&action.kind()) {
                return Err(AgentError::new(
                    "action_not_approved",
                    "该手势不在本次授权范围内。",
                ));
            }
            if frame.window.process_id != grant.target.process_id
                || frame.window.process_started != grant.target.process_started
                || (!grant.includes_file_dialogs && frame.window_id != grant.target_window_id)
            {
                return Err(AgentError::new(
                    "window_not_approved",
                    "目标窗口不在本次授权范围内。",
                ));
            }
            Ok(())
        })();
        if let Err(error) = validation {
            if matches!(
                error.code.as_str(),
                "document_changed"
                    | "computer_action_limit"
                    | "plan_changed"
                    | "action_not_approved"
                    | "window_not_approved"
            ) {
                revoke_grant(&mut state, grant_id);
            }
            return Err(error);
        }
        if let Err(error) =
            perform_platform_action(&frame.window, frame.geometry, frame.last_input_tick, action)
        {
            if matches!(
                error.code.as_str(),
                "input_outcome_unknown" | "stale_window"
            ) {
                revoke_grant(&mut state, grant_id);
            }
            return Err(error);
        }
        if settle_ms > 0 {
            std::thread::sleep(Duration::from_millis(settle_ms));
        }
        if let Some(grant) = state.grant.as_mut() {
            grant.action_count += 1;
        }
        let window_id = frame.window_id.clone();
        let window = frame.window.clone();
        state.frames.remove(frame_id);
        let next_frame_id = new_id();
        let operation_dir = cache_root.join("computer-operation").join(grant_id);
        let capture = match capture_window(&window, &operation_dir, &next_frame_id) {
            Ok(capture) => capture,
            Err(_) => {
                revoke_grant(&mut state, grant_id);
                return Err(AgentError::new(
                    "input_outcome_unknown",
                    "手势已发送但无法确认最新画面，操作结果未知，已停止后续操作。",
                ));
            }
        };
        state.frames.insert(
            next_frame_id.clone(),
            FrameRecord {
                grant_id: grant_id.to_string(),
                window_id: window_id.clone(),
                window: window.clone(),
                geometry: capture.geometry,
                last_input_tick: capture.last_input_tick,
            },
        );
        Ok(CapturedComputerFrame {
            frame: ComputerFrame {
                frame_id: next_frame_id,
                window_id,
                title: window.title,
                width: capture.geometry.width,
                height: capture.geometry.height,
            },
            path: capture.path,
        })
    }

    pub fn finish(
        &self,
        conversation_id: &str,
        grant_id: &str,
        outcome: ComputerOperationOutcome,
    ) -> Result<Value, AgentError> {
        let mut state = self.inner.lock().unwrap();
        let grant = require_grant_for_conversation(&state, grant_id, conversation_id)?;
        let action_count = grant.action_count;
        revoke_grant(&mut state, grant_id);
        Ok(json!({
            "outcome": outcome,
            "actionCount": action_count,
        }))
    }

    pub fn revoke_grant_for_conversation(&self, conversation_id: &str) {
        let mut state = self.inner.lock().unwrap();
        if state
            .grant
            .as_ref()
            .is_some_and(|grant| grant.conversation_id == conversation_id)
        {
            if let Some(id) = state.grant.as_ref().map(|grant| grant.id.clone()) {
                revoke_grant(&mut state, &id);
            }
        }
    }

    pub fn cancel_conversation(&self, conversation_id: &str) {
        let mut state = self.inner.lock().unwrap();
        state
            .pending
            .retain(|_, pending| pending.approval.conversation_id != conversation_id);
        if let Some(id) = state
            .grant
            .as_ref()
            .filter(|grant| grant.conversation_id == conversation_id)
            .map(|grant| grant.id.clone())
        {
            revoke_grant(&mut state, &id);
        }
    }
}

fn validate_plan(
    goal: &str,
    steps: &[ComputerOperationStep],
    allowed_actions: &[ComputerActionKind],
) -> Result<(), AgentError> {
    if goal.trim().is_empty() || goal.chars().count() > 500 {
        return Err(AgentError::new(
            "invalid_arguments",
            "操作目标不能为空且不能超过 500 个字符。",
        ));
    }
    if steps.is_empty() || steps.len() > 12 {
        return Err(AgentError::new(
            "invalid_arguments",
            "操作计划必须包含 1 到 12 个步骤。",
        ));
    }
    let ids = steps
        .iter()
        .map(|step| step.id.trim())
        .collect::<BTreeSet<_>>();
    if ids.len() != steps.len()
        || steps
            .iter()
            .any(|step| step.id.trim().is_empty() || step.title.trim().is_empty())
    {
        return Err(AgentError::new(
            "invalid_arguments",
            "操作步骤必须具有唯一 ID 和可读标题。",
        ));
    }
    if allowed_actions.is_empty() {
        return Err(AgentError::new(
            "invalid_arguments",
            "授权计划必须声明允许的手势。",
        ));
    }
    Ok(())
}

fn require_grant<'a>(
    state: &'a ComputerControlState,
    grant_id: &str,
) -> Result<&'a ComputerGrant, AgentError> {
    state
        .grant
        .as_ref()
        .filter(|grant| grant.id == grant_id && grant.expires_at > Instant::now())
        .ok_or_else(|| AgentError::new("grant_not_found", "电脑代理授权已失效。"))
}

fn require_grant_for_conversation<'a>(
    state: &'a ComputerControlState,
    grant_id: &str,
    conversation_id: &str,
) -> Result<&'a ComputerGrant, AgentError> {
    let grant = require_grant(state, grant_id)?;
    if grant.conversation_id != conversation_id {
        return Err(AgentError::new(
            "grant_not_found",
            "电脑代理授权不属于当前对话。",
        ));
    }
    Ok(grant)
}

fn verify_document(
    grant: &ComputerGrant,
    current_document_instance_key: Option<&str>,
) -> Result<(), AgentError> {
    if let Some(expected) = grant.document_instance_key.as_deref() {
        if current_document_instance_key != Some(expected) {
            return Err(AgentError::new(
                "document_changed",
                "当前 Cubism 文档已经变化，需要重新授权。",
            ));
        }
    }
    Ok(())
}

fn select_window(
    state: &ComputerControlState,
    grant: &ComputerGrant,
    requested: Option<&str>,
) -> Result<(String, PlatformWindow), AgentError> {
    let selected = requested.unwrap_or(&grant.target_window_id);
    let window = state
        .windows
        .get(selected)
        .cloned()
        .or_else(|| (selected == grant.target_window_id).then(|| grant.target.clone()))
        .ok_or_else(|| AgentError::new("stale_window", "目标窗口已失效。"))?;
    if window.process_id != grant.target.process_id
        || window.process_started != grant.target.process_started
        || (!grant.includes_file_dialogs && selected != grant.target_window_id)
    {
        return Err(AgentError::new(
            "window_not_approved",
            "目标窗口不在本次授权范围内。",
        ));
    }
    Ok((selected.to_string(), window))
}

fn expire_state(state: &mut ComputerControlState) {
    let now = Instant::now();
    state.pending.retain(|_, pending| pending.expires_at > now);
    if state
        .grant
        .as_ref()
        .is_some_and(|grant| grant.expires_at <= now)
    {
        if let Some(id) = state.grant.as_ref().map(|grant| grant.id.clone()) {
            revoke_grant(state, &id);
        }
    }
}

fn revoke_grant(state: &mut ComputerControlState, grant_id: &str) {
    state.frames.retain(|_, frame| frame.grant_id != grant_id);
    if state
        .grant
        .as_ref()
        .is_some_and(|grant| grant.id == grant_id)
    {
        if let Some(grant) = state.grant.take() {
            if let Some(path) = grant.cache_dir {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
}

#[cfg(windows)]
fn enumerate_windows(filter: Option<(u32, u64)>) -> Result<Vec<PlatformWindow>, AgentError> {
    windows::enumerate_windows(filter)
}

#[cfg(not(windows))]
fn enumerate_windows(_filter: Option<(u32, u64)>) -> Result<Vec<PlatformWindow>, AgentError> {
    Err(AgentError::new(
        "unsupported_platform",
        "当前平台不支持电脑代理操作。",
    ))
}

#[cfg(windows)]
fn window_is_current(window: &PlatformWindow) -> bool {
    windows::window_is_current(window)
}

#[cfg(not(windows))]
fn window_is_current(_window: &PlatformWindow) -> bool {
    false
}

#[cfg(windows)]
fn capture_window(
    window: &PlatformWindow,
    cache_dir: &Path,
    frame_id: &str,
) -> Result<PlatformCapture, AgentError> {
    windows::capture_window(window, cache_dir, frame_id)
}

#[cfg(not(windows))]
fn capture_window(
    _window: &PlatformWindow,
    _cache_dir: &Path,
    _frame_id: &str,
) -> Result<PlatformCapture, AgentError> {
    Err(AgentError::new(
        "unsupported_platform",
        "当前平台不支持电脑代理操作。",
    ))
}

#[cfg(windows)]
fn perform_platform_action(
    window: &PlatformWindow,
    geometry: PlatformGeometry,
    last_input_tick: u32,
    action: &ComputerAction,
) -> Result<(), AgentError> {
    windows::perform_action(window, geometry, last_input_tick, action)
}

#[cfg(not(windows))]
fn perform_platform_action(
    _window: &PlatformWindow,
    _geometry: PlatformGeometry,
    _last_input_tick: u32,
    _action: &ComputerAction,
) -> Result<(), AgentError> {
    Err(AgentError::new(
        "unsupported_platform",
        "当前平台不支持电脑代理操作。",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_window() -> PlatformWindow {
        PlatformWindow {
            handle: 1,
            process_id: 2,
            process_started: 3,
            title: "Cubism Editor".into(),
            width: 800,
            height: 600,
        }
    }

    fn approval(action_id: &str) -> ComputerApproval {
        ComputerApproval {
            action_id: action_id.into(),
            conversation_id: "conversation".into(),
            goal: "调整控制点".into(),
            reason: "没有可用 API".into(),
            target_window_title: "Cubism Editor".into(),
            steps: vec![ComputerOperationStep {
                id: "move".into(),
                title: "拖动控制点".into(),
            }],
            allowed_actions: vec![ComputerActionKind::Drag],
            includes_file_dialogs: false,
            impact: "将注入输入".into(),
            cannot_undo: true,
            expires_at: Utc::now().to_rfc3339(),
        }
    }

    fn install_fake_grant_and_frame(
        service: &ComputerControlService,
        coordinator: &OperationCoordinator,
        frame_window: PlatformWindow,
    ) {
        let permit = coordinator
            .try_acquire(OperationOwnerKind::ComputerControl, "grant")
            .unwrap();
        let mut state = service.inner.lock().unwrap();
        state.grant = Some(ComputerGrant {
            id: "grant".into(),
            conversation_id: "conversation".into(),
            target_window_id: "window".into(),
            target: fake_window(),
            allowed_actions: BTreeSet::from([ComputerActionKind::Drag]),
            step_ids: BTreeSet::from(["move".into()]),
            includes_file_dialogs: false,
            document_instance_key: None,
            expires_at: Instant::now() + Duration::from_secs(10),
            action_count: 0,
            cache_dir: None,
            _permit: permit,
        });
        state.frames.insert(
            "frame".into(),
            FrameRecord {
                grant_id: "grant".into(),
                window_id: "window".into(),
                window: frame_window,
                geometry: PlatformGeometry {
                    screen_x: 0,
                    screen_y: 0,
                    width: 800,
                    height: 600,
                    dpi: 96,
                },
                last_input_tick: 0,
            },
        );
    }

    #[test]
    fn fallback_registry_only_contains_confirmed_missing_api_categories() {
        let categories = [
            UnsupportedCapability::ArtMeshGeometry,
            UnsupportedCapability::ArtMeshUvTopology,
            UnsupportedCapability::WarpControlPoints,
            UnsupportedCapability::AnimationEditing,
            UnsupportedCapability::PhysicsEditing,
            UnsupportedCapability::SaveExport,
            UnsupportedCapability::TextureAtlas,
            UnsupportedCapability::PsdOperations,
            UnsupportedCapability::GlueCreation,
            UnsupportedCapability::ArtPath,
        ];
        assert_eq!(categories.len(), 10);
        assert!(categories
            .iter()
            .all(|category| !category.label().is_empty()));
        assert!(!UnsupportedCapability::SaveExport.available_in_first_release());
        assert!(!UnsupportedCapability::TextureAtlas.available_in_first_release());
        assert!(!UnsupportedCapability::PsdOperations.available_in_first_release());
        assert!(UnsupportedCapability::WarpControlPoints.available_in_first_release());
    }

    #[test]
    fn plans_require_unique_steps_and_explicit_gestures() {
        let step = ComputerOperationStep {
            id: "inspect".into(),
            title: "检查界面".into(),
        };
        assert!(validate_plan(
            "调整控制点",
            std::slice::from_ref(&step),
            &[ComputerActionKind::Drag]
        )
        .is_ok());
        assert!(validate_plan(
            "调整控制点",
            &[step.clone(), step],
            &[ComputerActionKind::Drag]
        )
        .is_err());
        assert!(validate_plan("调整控制点", &[], &[ComputerActionKind::Drag]).is_err());
        assert!(validate_plan(
            "调整控制点",
            &[ComputerOperationStep {
                id: "x".into(),
                title: "步骤".into()
            }],
            &[]
        )
        .is_err());
    }

    #[test]
    fn protected_calls_reject_missing_or_replayed_grants() {
        let service = ComputerControlService::new(OperationCoordinator::default());
        let error = service
            .capture_frame("conversation", "missing", None, Path::new("."), None)
            .err()
            .unwrap();
        assert_eq!(error.code, "grant_not_found");
        assert!(matches!(
            service.finish(
                "conversation",
                "missing",
                ComputerOperationOutcome::Completed
            ),
            Err(error) if error.code == "grant_not_found"
        ));
    }

    #[test]
    fn rejection_and_expiration_never_create_a_grant() {
        let service = ComputerControlService::new(OperationCoordinator::default());
        {
            let mut state = service.inner.lock().unwrap();
            state.pending.insert(
                "reject".into(),
                PendingOperation {
                    approval: approval("reject"),
                    target_window_id: "window".into(),
                    target: fake_window(),
                    capability: UnsupportedCapability::WarpControlPoints,
                    allowed_actions: BTreeSet::from([ComputerActionKind::Drag]),
                    step_ids: BTreeSet::from(["move".into()]),
                    document_instance_key: Some("document".into()),
                    expires_at: Instant::now() + Duration::from_secs(10),
                },
            );
        }
        assert_eq!(service.decide("reject", false).unwrap()["approved"], false);
        assert!(service.decide("reject", true).is_err());
        assert!(!service.has_active_grant("conversation"));

        {
            let mut state = service.inner.lock().unwrap();
            state.pending.insert(
                "expired".into(),
                PendingOperation {
                    approval: approval("expired"),
                    target_window_id: "window".into(),
                    target: fake_window(),
                    capability: UnsupportedCapability::WarpControlPoints,
                    allowed_actions: BTreeSet::from([ComputerActionKind::Drag]),
                    step_ids: BTreeSet::from(["move".into()]),
                    document_instance_key: None,
                    expires_at: Instant::now() - Duration::from_secs(1),
                },
            );
        }
        assert!(service.pending_approval("expired").is_none());
    }

    #[test]
    fn document_change_and_finish_revoke_the_ephemeral_grant() {
        let coordinator = OperationCoordinator::default();
        let service = ComputerControlService::new(coordinator.clone());
        let permit = coordinator
            .try_acquire(OperationOwnerKind::ComputerControl, "grant")
            .unwrap();
        let cache_dir = std::env::temp_dir().join(format!("computer-grant-{}", new_id()));
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("frame.png"), b"temporary").unwrap();
        {
            let mut state = service.inner.lock().unwrap();
            state.grant = Some(ComputerGrant {
                id: "grant".into(),
                conversation_id: "conversation".into(),
                target_window_id: "window".into(),
                target: fake_window(),
                allowed_actions: BTreeSet::from([ComputerActionKind::Drag]),
                step_ids: BTreeSet::from(["move".into()]),
                includes_file_dialogs: false,
                document_instance_key: Some("document".into()),
                expires_at: Instant::now() + Duration::from_secs(10),
                action_count: 2,
                cache_dir: Some(cache_dir.clone()),
                _permit: permit,
            });
        }
        {
            let state = service.inner.lock().unwrap();
            let grant = state.grant.as_ref().unwrap();
            assert!(matches!(
                verify_document(grant, Some("changed")),
                Err(error) if error.code == "document_changed"
            ));
        }
        let result = service
            .finish(
                "conversation",
                "grant",
                ComputerOperationOutcome::NeedsUserVerification,
            )
            .unwrap();
        assert_eq!(result["actionCount"], 2);
        assert!(!cache_dir.exists());
        assert!(coordinator
            .try_acquire(OperationOwnerKind::EditorTransaction, "editor")
            .is_ok());
    }

    #[test]
    fn plan_and_process_changes_stop_before_input_injection() {
        let coordinator = OperationCoordinator::default();
        let service = ComputerControlService::new(coordinator.clone());
        let mut changed_process = fake_window();
        changed_process.process_id += 1;
        install_fake_grant_and_frame(&service, &coordinator, changed_process);
        let action = ComputerAction::Drag {
            from_x: 1,
            from_y: 1,
            to_x: 2,
            to_y: 2,
            duration_ms: 100,
        };
        let cancel = AtomicBool::new(false);
        assert!(matches!(
            service.perform_action(
                "conversation",
                "grant",
                "frame",
                "changed-step",
                &action,
                0,
                Path::new("."),
                None,
                &cancel,
            ),
            Err(error) if error.code == "plan_changed"
        ));
        let mut changed_process = fake_window();
        changed_process.process_id += 1;
        install_fake_grant_and_frame(&service, &coordinator, changed_process);
        assert!(matches!(
            service.perform_action(
                "conversation",
                "grant",
                "frame",
                "move",
                &action,
                0,
                Path::new("."),
                None,
                &cancel,
            ),
            Err(error) if error.code == "window_not_approved"
        ));
        service.cancel_conversation("conversation");
    }
}
