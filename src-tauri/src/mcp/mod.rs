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
use tools::McpToolContext;

struct RunningServer {
    task: HttpServerTask,
    port: u16,
}

pub struct McpServerHandle {
    data_dir: PathBuf,
    config: Mutex<McpConfig>,
    token: Arc<Mutex<String>>,
    run_state: Mutex<McpRunState>,
    message: Mutex<String>,
    running: Mutex<Option<RunningServer>>,
    psd: Arc<PsdService>,
    psd_documents: Arc<Mutex<Vec<ChatPsdDocument>>>,
    allow_writes: Arc<Mutex<bool>>,
}

impl McpServerHandle {
    pub fn new(data_dir: PathBuf) -> Result<Self, AgentError> {
        let config = load_config(&data_dir)?;
        let allow_writes = Arc::new(Mutex::new(config.allow_writes));
        Ok(Self {
            psd: Arc::new(PsdService::new(Some(data_dir.clone()))),
            data_dir,
            config: Mutex::new(config),
            token: Arc::new(Mutex::new(load_token()?)),
            run_state: Mutex::new(McpRunState::Stopped),
            message: Mutex::new("MCP 未启动。".into()),
            running: Mutex::new(None),
            psd_documents: Arc::new(Mutex::new(Vec::new())),
            allow_writes,
        })
    }

    pub fn status(&self) -> Result<McpStatus, AgentError> {
        let config = self.config.lock().unwrap().clone();
        let state = *self.run_state.lock().unwrap();
        Ok(McpStatus {
            state,
            enabled: config.enabled,
            port: config.port,
            allow_writes: config.allow_writes,
            url: matches!(state, McpRunState::Running).then(|| mcp_url(config.port)),
            token: self.token.lock().unwrap().clone(),
            message: self.message.lock().unwrap().clone(),
        })
    }

    pub async fn set_config(
        &self,
        app: &AppHandle,
        input: McpConfigInput,
    ) -> Result<McpStatus, AgentError> {
        let next = apply_input(self.config.lock().unwrap().clone(), input)?;
        save_config(&self.data_dir, &next)?;
        *self.allow_writes.lock().unwrap() = next.allow_writes;
        let running_port = self.running.lock().unwrap().as_ref().map(|r| r.port);
        let was_running = running_port.is_some();
        *self.config.lock().unwrap() = next.clone();

        let need_restart =
            was_running && (!next.enabled || running_port != Some(next.port));
        if need_restart {
            self.stop_inner(app).await?;
        }
        if next.enabled && (!was_running || need_restart) {
            self.start_inner(app).await?;
        } else if !need_restart {
            self.emit_status(app)?;
        }
        self.status()
    }

    pub async fn start(&self, app: &AppHandle) -> Result<McpStatus, AgentError> {
        {
            let mut config = self.config.lock().unwrap();
            config.enabled = true;
            save_config(&self.data_dir, &config)?;
        }
        self.start_inner(app).await?;
        self.status()
    }

    pub async fn stop(&self, app: &AppHandle) -> Result<McpStatus, AgentError> {
        {
            let mut config = self.config.lock().unwrap();
            config.enabled = false;
            save_config(&self.data_dir, &config)?;
        }
        self.stop_inner(app).await?;
        self.status()
    }

    pub fn rotate_token(&self, app: &AppHandle) -> Result<McpStatus, AgentError> {
        *self.token.lock().unwrap() = rotate_token()?;
        self.emit_status(app)?;
        self.status()
    }

    pub async fn restore_if_enabled(&self, app: &AppHandle) {
        if self.config.lock().unwrap().enabled {
            let _ = self.start_inner(app).await;
        }
    }

    async fn start_inner(&self, app: &AppHandle) -> Result<(), AgentError> {
        if matches!(*self.run_state.lock().unwrap(), McpRunState::Running) {
            return Ok(());
        }
        self.set_state(app, McpRunState::Starting, "正在启动 MCP…")?;
        let port = self.config.lock().unwrap().port;
        let context = McpToolContext {
            app: app.clone(),
            editor: (*app.state::<EditorService>()).clone(),
            psd: self.psd.clone(),
            psd_documents: self.psd_documents.clone(),
            allow_writes: self.allow_writes.clone(),
        };
        match http::bind_and_serve(port, self.token.clone(), context).await {
            Ok(task) => {
                *self.running.lock().unwrap() = Some(RunningServer { task, port });
                self.set_state(
                    app,
                    McpRunState::Running,
                    format!("MCP 已在 {} 监听。", mcp_url(port)),
                )
            }
            Err(message) => {
                self.set_state(app, McpRunState::Failed, message.clone())?;
                Err(AgentError::new("mcp_bind_failed", message))
            }
        }
    }

    async fn stop_inner(&self, app: &AppHandle) -> Result<(), AgentError> {
        let running = self.running.lock().unwrap().take();
        if let Some(running) = running {
            running.task.cancellation.cancel();
            let _ = running.task.join.await;
        }
        self.set_state(app, McpRunState::Stopped, "MCP 已停止。")
    }

    fn set_state(
        &self,
        app: &AppHandle,
        state: McpRunState,
        message: impl Into<String>,
    ) -> Result<(), AgentError> {
        *self.run_state.lock().unwrap() = state;
        *self.message.lock().unwrap() = message.into();
        self.emit_status(app)
    }

    fn emit_status(&self, app: &AppHandle) -> Result<(), AgentError> {
        let _ = app.emit(MCP_STATE_EVENT, &self.status()?);
        Ok(())
    }
}

fn mcp_handle(app: &AppHandle) -> Result<Arc<McpServerHandle>, AgentError> {
    app.try_state::<Arc<McpServerHandle>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| AgentError::new("mcp_missing", "MCP 服务未初始化。"))
}

#[tauri::command]
pub async fn mcp_get_status(app: AppHandle) -> Result<McpStatus, AgentError> {
    mcp_handle(&app)?.status()
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
    mcp_handle(&app)?.rotate_token(&app)
}
