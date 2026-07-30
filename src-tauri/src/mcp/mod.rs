mod config;
mod http;
mod tools;

pub use config::{McpConfigInput, McpRunState, McpStatus};

use crate::agent::psd::{ChatPsdDocument, PsdService};
use crate::agent::AgentError;
use crate::service::EditorService;
use config::{
    apply_input, load_config, load_token, mcp_url, rotate_token, save_config, McpConfig,
    MCP_STATE_EVENT,
};
use http::HttpServerTask;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tools::McpToolContext;

struct RunningServer {
    generation: u64,
    port: u16,
    cancellation: tokio_util::sync::CancellationToken,
    completion: oneshot::Receiver<()>,
}

struct LifecycleState {
    generation: u64,
    desired: McpConfig,
    phase: McpRunState,
    message: String,
    running: Option<RunningServer>,
}

pub struct McpServerHandle {
    data_dir: PathBuf,
    token: Arc<Mutex<String>>,
    lifecycle: AsyncMutex<LifecycleState>,
    psd: Arc<PsdService>,
    psd_documents: Arc<Mutex<Vec<ChatPsdDocument>>>,
    allow_writes: Arc<Mutex<bool>>,
    #[cfg(test)]
    before_bind: Mutex<Option<Arc<tokio::sync::Barrier>>>,
    #[cfg(test)]
    bind_count: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    bind_entered: tokio::sync::Notify,
    #[cfg(test)]
    published_states: Mutex<Vec<McpRunState>>,
}

impl McpServerHandle {
    pub fn new(data_dir: PathBuf) -> Result<Self, AgentError> {
        let config = load_config(&data_dir)?;
        let token = load_token()?;
        Ok(Self::from_parts(data_dir, config, token))
    }

    fn from_parts(data_dir: PathBuf, config: McpConfig, token: String) -> Self {
        let allow_writes = Arc::new(Mutex::new(config.allow_writes));
        let psd = Arc::new(PsdService::new(Some(data_dir.clone())));
        if let Err(error) = psd.delete_conversation_psds(tools::MCP_PSD_SESSION) {
            eprintln!("MCP PSD startup cleanup failed: {}", error.message);
        }
        Self {
            psd,
            data_dir,
            token: Arc::new(Mutex::new(token)),
            lifecycle: AsyncMutex::new(LifecycleState {
                generation: 0,
                desired: config,
                phase: McpRunState::Stopped,
                message: "MCP 未启动。".into(),
                running: None,
            }),
            psd_documents: Arc::new(Mutex::new(Vec::new())),
            allow_writes,
            #[cfg(test)]
            before_bind: Mutex::new(None),
            #[cfg(test)]
            bind_count: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            bind_entered: tokio::sync::Notify::new(),
            #[cfg(test)]
            published_states: Mutex::new(Vec::new()),
        }
    }

    pub async fn status(&self) -> McpStatus {
        let state = self.lifecycle.lock().await;
        self.status_from(&state)
    }

    fn status_from(&self, state: &LifecycleState) -> McpStatus {
        McpStatus {
            state: state.phase,
            enabled: state.desired.enabled,
            port: state.desired.port,
            allow_writes: state.desired.allow_writes,
            url: matches!(state.phase, McpRunState::Running).then(|| mcp_url(state.desired.port)),
            token: self.token.lock().unwrap().clone(),
            message: state.message.clone(),
        }
    }

    pub async fn set_config(
        self: &Arc<Self>,
        app: &AppHandle,
        input: McpConfigInput,
    ) -> Result<McpStatus, AgentError> {
        let mut state = self.lifecycle.lock().await;
        let next = apply_input(state.desired.clone(), input)?;
        save_config(&self.data_dir, &next)?;
        *self.allow_writes.lock().unwrap() = next.allow_writes;
        let running_port = state.running.as_ref().map(|running| running.port);
        let need_stop =
            running_port.is_some() && (!next.enabled || running_port != Some(next.port));
        state.desired = next;
        if need_stop {
            let restarting = state.desired.enabled;
            self.stop_locked(Some(app), &mut state, !restarting).await;
        }
        if state.desired.enabled && state.running.is_none() {
            self.start_locked(app, &mut state).await?;
        } else if !state.desired.enabled && state.running.is_none() {
            state.phase = McpRunState::Stopped;
            state.message = "MCP 已停止。".into();
            self.emit_status(Some(app), &state);
        } else {
            self.emit_status(Some(app), &state);
        }
        Ok(self.status_from(&state))
    }

    pub async fn start(self: &Arc<Self>, app: &AppHandle) -> Result<McpStatus, AgentError> {
        let mut state = self.lifecycle.lock().await;
        state.desired.enabled = true;
        save_config(&self.data_dir, &state.desired)?;
        self.start_locked(app, &mut state).await?;
        Ok(self.status_from(&state))
    }

    pub async fn stop(self: &Arc<Self>, app: &AppHandle) -> Result<McpStatus, AgentError> {
        let mut state = self.lifecycle.lock().await;
        state.desired.enabled = false;
        save_config(&self.data_dir, &state.desired)?;
        self.stop_locked(Some(app), &mut state, true).await;
        Ok(self.status_from(&state))
    }

    pub async fn rotate_token(&self, app: &AppHandle) -> Result<McpStatus, AgentError> {
        *self.token.lock().unwrap() = rotate_token()?;
        let state = self.lifecycle.lock().await;
        self.emit_status(Some(app), &state);
        Ok(self.status_from(&state))
    }

    pub async fn restore_if_enabled(self: &Arc<Self>, app: &AppHandle) {
        let mut state = self.lifecycle.lock().await;
        if state.desired.enabled {
            let _ = self.start_locked(app, &mut state).await;
        }
    }

    async fn start_locked(
        self: &Arc<Self>,
        app: &AppHandle,
        state: &mut LifecycleState,
    ) -> Result<(), AgentError> {
        if state.running.is_some() {
            return Ok(());
        }
        self.cleanup_mcp_psds();
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.phase = McpRunState::Starting;
        state.message = "正在启动 MCP…".into();
        self.emit_status(Some(app), state);
        let port = state.desired.port;
        let context = McpToolContext {
            app: app.clone(),
            editor: (*app.state::<EditorService>()).clone(),
            psd: self.psd.clone(),
            psd_documents: self.psd_documents.clone(),
            allow_writes: self.allow_writes.clone(),
        };
        match http::bind_and_serve(port, self.token.clone(), context).await {
            Ok(task) => {
                self.install_running_server(Some(app), state, generation, port, task);
                Ok(())
            }
            Err(message) => {
                state.phase = McpRunState::Failed;
                state.message = message.clone();
                self.emit_status(Some(app), state);
                Err(AgentError::new("mcp_bind_failed", message))
            }
        }
    }

    fn install_running_server(
        self: &Arc<Self>,
        app: Option<&AppHandle>,
        state: &mut LifecycleState,
        generation: u64,
        port: u16,
        task: HttpServerTask,
    ) {
        let (completed_tx, completed_rx) = oneshot::channel();
        let cancellation = task.cancellation.clone();
        state.running = Some(RunningServer {
            generation,
            port,
            cancellation,
            completion: completed_rx,
        });
        state.phase = McpRunState::Running;
        state.message = format!("MCP 已在 {} 监听。", mcp_url(port));
        self.emit_status(app, state);

        let handle = Arc::downgrade(self);
        let app = app.cloned();
        tokio::spawn(async move {
            let result = match task.join.await {
                Ok(result) => result,
                Err(error) => Err(format!("MCP Server 任务退出：{error}")),
            };
            let _ = completed_tx.send(());
            if let Some(handle) = handle.upgrade() {
                handle.server_exited(app, generation, result).await;
            }
        });
    }

    async fn stop_locked(
        &self,
        app: Option<&AppHandle>,
        state: &mut LifecycleState,
        publish_stopped: bool,
    ) {
        state.generation = state.generation.wrapping_add(1);
        let running = state.running.take();
        if let Some(running) = running {
            state.phase = McpRunState::Stopping;
            state.message = "正在停止 MCP…".into();
            self.emit_status(app, state);
            running.cancellation.cancel();
            let _ = running.completion.await;
        }
        self.cleanup_mcp_psds();
        state.phase = McpRunState::Stopped;
        state.message = "MCP 已停止。".into();
        if publish_stopped {
            self.emit_status(app, state);
        }
    }

    async fn server_exited(
        &self,
        app: Option<AppHandle>,
        generation: u64,
        result: Result<(), String>,
    ) {
        let mut state = self.lifecycle.lock().await;
        if state.generation != generation
            || state
                .running
                .as_ref()
                .is_none_or(|running| running.generation != generation)
        {
            return;
        }
        state.running = None;
        state.phase = McpRunState::Failed;
        state.message = result
            .err()
            .unwrap_or_else(|| "MCP Server 已意外退出。".into());
        self.emit_status(app.as_ref(), &state);
    }

    fn cleanup_mcp_psds(&self) {
        if let Err(error) = self.psd.delete_conversation_psds(tools::MCP_PSD_SESSION) {
            eprintln!("MCP PSD cleanup failed: {}", error.message);
        }
        self.psd_documents.lock().unwrap().clear();
    }

    fn emit_status(&self, app: Option<&AppHandle>, state: &LifecycleState) {
        #[cfg(test)]
        self.published_states.lock().unwrap().push(state.phase);
        if let Some(app) = app {
            let _ = app.emit(MCP_STATE_EVENT, self.status_from(state));
        }
    }

    #[cfg(test)]
    fn pause_next_bind(&self, barrier: Arc<tokio::sync::Barrier>) {
        *self.before_bind.lock().unwrap() = Some(barrier);
    }

    #[cfg(test)]
    async fn start_for_test(self: &Arc<Self>) -> Result<(), AgentError> {
        use std::sync::atomic::Ordering;

        let mut state = self.lifecycle.lock().await;
        state.desired.enabled = true;
        if state.running.is_some() {
            return Ok(());
        }
        self.cleanup_mcp_psds();
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        let port = state.desired.port;
        state.phase = McpRunState::Starting;
        state.message = "正在启动 MCP…".into();
        self.emit_status(None, &state);
        self.bind_count.fetch_add(1, Ordering::SeqCst);
        let barrier = { self.before_bind.lock().unwrap().take() };
        if let Some(barrier) = barrier {
            self.bind_entered.notify_one();
            barrier.wait().await;
        }
        let task = http::bind_test_server(port)
            .await
            .map_err(|message| AgentError::new("mcp_bind_failed", message))?;
        self.install_running_server(None, &mut state, generation, port, task);
        Ok(())
    }

    #[cfg(test)]
    async fn stop_for_test(&self) {
        let mut state = self.lifecycle.lock().await;
        state.desired.enabled = false;
        self.stop_locked(None, &mut state, true).await;
    }

    #[cfg(test)]
    async fn set_port_for_test(self: &Arc<Self>, port: u16) -> Result<(), AgentError> {
        let mut state = self.lifecycle.lock().await;
        let running_port = state.running.as_ref().map(|running| running.port);
        state.desired.enabled = true;
        state.desired.port = port;
        save_config(&self.data_dir, &state.desired)?;
        if running_port.is_some_and(|running_port| running_port != port) {
            self.stop_locked(None, &mut state, false).await;
        }
        drop(state);
        self.start_for_test().await
    }

    #[cfg(test)]
    async fn cancel_running_for_test(&self) {
        let cancellation = self
            .lifecycle
            .lock()
            .await
            .running
            .as_ref()
            .map(|running| running.cancellation.clone());
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        }
    }
}

impl Drop for McpServerHandle {
    fn drop(&mut self) {
        if let Ok(state) = self.lifecycle.try_lock() {
            if let Some(running) = state.running.as_ref() {
                running.cancellation.cancel();
            }
        }
        self.cleanup_mcp_psds();
    }
}

fn mcp_handle(app: &AppHandle) -> Result<Arc<McpServerHandle>, AgentError> {
    app.try_state::<Arc<McpServerHandle>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| AgentError::new("mcp_missing", "MCP 服务未初始化。"))
}

#[tauri::command]
pub async fn mcp_get_status(app: AppHandle) -> Result<McpStatus, AgentError> {
    Ok(mcp_handle(&app)?.status().await)
}

#[tauri::command]
pub async fn mcp_set_config(
    app: AppHandle,
    input: McpConfigInput,
) -> Result<McpStatus, AgentError> {
    mcp_handle(&app)?.set_config(&app, input).await
}

#[tauri::command]
pub async fn mcp_start(app: AppHandle) -> Result<McpStatus, AgentError> {
    mcp_handle(&app)?.start(&app).await
}

#[tauri::command]
pub async fn mcp_stop(app: AppHandle) -> Result<McpStatus, AgentError> {
    mcp_handle(&app)?.stop(&app).await
}

#[tauri::command]
pub async fn mcp_rotate_token(app: AppHandle) -> Result<McpStatus, AgentError> {
    mcp_handle(&app)?.rotate_token(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener as StdTcpListener;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nbc-mcp-{}", crate::agent::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn free_ports(count: usize) -> Vec<u16> {
        let listeners = (0..count)
            .map(|_| StdTcpListener::bind("127.0.0.1:0").unwrap())
            .collect::<Vec<_>>();
        listeners
            .iter()
            .map(|listener| listener.local_addr().unwrap().port())
            .collect()
    }

    fn free_port() -> u16 {
        free_ports(1)[0]
    }

    fn handle(data_dir: PathBuf, port: u16) -> Arc<McpServerHandle> {
        Arc::new(McpServerHandle::from_parts(
            data_dir,
            McpConfig {
                enabled: false,
                port,
                allow_writes: true,
            },
            "test-token".into(),
        ))
    }

    async fn wait_for_state(handle: &McpServerHandle, expected: McpRunState) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if handle.status().await.state == expected {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn concurrent_starts_bind_once() {
        let dir = test_dir();
        let handle = handle(dir.clone(), free_port());
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        handle.pause_next_bind(barrier.clone());

        let first = tokio::spawn({
            let handle = handle.clone();
            async move { handle.start_for_test().await }
        });
        let second = tokio::spawn({
            let handle = handle.clone();
            async move { handle.start_for_test().await }
        });
        handle.bind_entered.notified().await;
        barrier.wait().await;

        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(handle.status().await.state, McpRunState::Running);
        assert_eq!(handle.bind_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *handle.published_states.lock().unwrap(),
            [McpRunState::Starting, McpRunState::Running]
        );

        handle.stop_for_test().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn stop_accepted_during_start_wins_and_releases_the_port() {
        let dir = test_dir();
        let port = free_port();
        let handle = handle(dir.clone(), port);
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        handle.pause_next_bind(barrier.clone());
        let starting = tokio::spawn({
            let handle = handle.clone();
            async move { handle.start_for_test().await }
        });
        handle.bind_entered.notified().await;
        let stopping = tokio::spawn({
            let handle = handle.clone();
            async move { handle.stop_for_test().await }
        });
        barrier.wait().await;

        starting.await.unwrap().unwrap();
        stopping.await.unwrap();
        assert_eq!(handle.status().await.state, McpRunState::Stopped);
        assert_eq!(
            *handle.published_states.lock().unwrap(),
            [
                McpRunState::Starting,
                McpRunState::Running,
                McpRunState::Stopping,
                McpRunState::Stopped,
            ]
        );
        let released = StdTcpListener::bind(("127.0.0.1", port)).unwrap();
        drop(released);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn serialized_port_changes_leave_only_the_last_listener() {
        let dir = test_dir();
        let ports = free_ports(3);
        let [first_port, second_port, final_port] = ports.as_slice() else {
            unreachable!()
        };
        let (first_port, second_port, final_port) = (*first_port, *second_port, *final_port);
        let handle = handle(dir.clone(), first_port);
        handle.start_for_test().await.unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        handle.pause_next_bind(barrier.clone());

        let first_change = tokio::spawn({
            let handle = handle.clone();
            async move { handle.set_port_for_test(second_port).await }
        });
        handle.bind_entered.notified().await;
        let final_change = tokio::spawn({
            let handle = handle.clone();
            async move { handle.set_port_for_test(final_port).await }
        });
        barrier.wait().await;
        first_change.await.unwrap().unwrap();
        final_change.await.unwrap().unwrap();

        let status = handle.status().await;
        assert_eq!(status.state, McpRunState::Running);
        assert_eq!(status.port, final_port);
        assert_eq!(load_config(&dir).unwrap().port, final_port);
        assert!(StdTcpListener::bind(("127.0.0.1", first_port)).is_ok());
        assert!(StdTcpListener::bind(("127.0.0.1", second_port)).is_ok());
        assert!(StdTcpListener::bind(("127.0.0.1", final_port)).is_err());

        handle.stop_for_test().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn unexpected_server_exit_marks_failed_and_can_restart() {
        let dir = test_dir();
        let handle = handle(dir.clone(), free_port());
        handle.start_for_test().await.unwrap();
        handle.cancel_running_for_test().await;
        wait_for_state(&handle, McpRunState::Failed).await;

        handle.start_for_test().await.unwrap();
        assert_eq!(handle.status().await.state, McpRunState::Running);
        handle.stop_for_test().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn normal_stop_clears_mcp_psd_files_and_index() {
        let dir = test_dir();
        let handle = handle(dir.clone(), free_port());
        handle.start_for_test().await.unwrap();
        let psd_path = dir.join("chat-psd/mcp/document.psd");
        let cache_path = dir.join("cache/psd-layers/mcp/document/0.png");
        for path in [&psd_path, &cache_path] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"data").unwrap();
        }
        handle.psd_documents.lock().unwrap().push(ChatPsdDocument {
            id: "document".into(),
            name: "document.psd".into(),
            path: psd_path.to_string_lossy().into_owned(),
            width: 1,
            height: 1,
            color_mode: "rgb".into(),
            layer_count: 1,
            available: true,
        });

        handle.stop_for_test().await;

        assert!(!psd_path.exists());
        assert!(!cache_path.exists());
        assert!(handle.psd_documents.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn startup_sweep_removes_only_mcp_psd_residue() {
        let dir = test_dir();
        let mcp_psd = dir.join("chat-psd/mcp/orphan.psd");
        let mcp_cache = dir.join("cache/psd-layers/mcp/orphan/0.png");
        let legacy_mcp_cache = dir.join("cache/psd-layers/orphan-0-legacy.png");
        let chat_psd = dir.join("chat-psd/chat/document.psd");
        let chat_cache = dir.join("cache/psd-layers/chat/document/0.png");
        for path in [
            &mcp_psd,
            &mcp_cache,
            &legacy_mcp_cache,
            &chat_psd,
            &chat_cache,
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"data").unwrap();
        }

        let handle = handle(dir.clone(), free_port());
        assert!(!mcp_psd.exists());
        assert!(!mcp_cache.exists());
        assert!(!legacy_mcp_cache.exists());
        assert!(chat_psd.is_file());
        assert!(chat_cache.is_file());

        drop(handle);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn handle_drop_cleans_mcp_psd_residue() {
        let dir = test_dir();
        let handle = handle(dir.clone(), free_port());
        let psd_path = dir.join("chat-psd/mcp/document.psd");
        let cache_path = dir.join("cache/psd-layers/mcp/document/0.png");
        for path in [&psd_path, &cache_path] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"data").unwrap();
        }

        drop(handle);

        assert!(!psd_path.exists());
        assert!(!cache_path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
