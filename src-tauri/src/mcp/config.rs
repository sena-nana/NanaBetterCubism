use crate::agent::AgentError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) const DEFAULT_MCP_PORT: u16 = 3920;
pub(crate) const MCP_STATE_EVENT: &str = "mcp://state";

const KEYRING_SERVICE: &str = "com.senanana.nanabettercubism";
const KEYRING_ACCOUNT: &str = "mcp-bearer-token";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    pub allow_writes: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_MCP_PORT,
            allow_writes: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigInput {
    pub enabled: Option<bool>,
    pub port: Option<u16>,
    pub allow_writes: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpRunState {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub state: McpRunState,
    pub enabled: bool,
    pub port: u16,
    pub allow_writes: bool,
    pub url: Option<String>,
    pub token: String,
    pub message: String,
}

pub(crate) fn load_config(data_dir: &Path) -> Result<McpConfig, AgentError> {
    let path = data_dir.join("mcp-config.json");
    if !path.is_file() {
        return Ok(McpConfig::default());
    }
    let bytes = std::fs::read(&path)
        .map_err(|error| AgentError::new("mcp_config_error", error.to_string()))?;
    let mut config: McpConfig = serde_json::from_slice(&bytes)
        .map_err(|error| AgentError::new("mcp_config_error", error.to_string()))?;
    if !(1..=65535).contains(&config.port) {
        config.port = DEFAULT_MCP_PORT;
    }
    Ok(config)
}

pub(crate) fn save_config(data_dir: &Path, config: &McpConfig) -> Result<(), AgentError> {
    if !(1..=65535).contains(&config.port) {
        return Err(AgentError::new(
            "invalid_arguments",
            "port 必须是 1 到 65535 的整数",
        ));
    }
    std::fs::create_dir_all(data_dir)
        .map_err(|error| AgentError::new("mcp_config_error", error.to_string()))?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| AgentError::new("mcp_config_error", error.to_string()))?;
    std::fs::write(data_dir.join("mcp-config.json"), bytes)
        .map_err(|error| AgentError::new("mcp_config_error", error.to_string()))
}

pub(crate) fn apply_input(
    current: McpConfig,
    input: McpConfigInput,
) -> Result<McpConfig, AgentError> {
    let mut next = current;
    if let Some(enabled) = input.enabled {
        next.enabled = enabled;
    }
    if let Some(port) = input.port {
        if !(1..=65535).contains(&port) {
            return Err(AgentError::new(
                "invalid_arguments",
                "port 必须是 1 到 65535 的整数",
            ));
        }
        next.port = port;
    }
    if let Some(allow_writes) = input.allow_writes {
        next.allow_writes = allow_writes;
    }
    Ok(next)
}

pub(crate) fn mcp_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

pub(crate) fn load_token() -> Result<String, AgentError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| AgentError::new("mcp_keyring_error", error.to_string()))?;
    match entry.get_password() {
        Ok(token) if !token.trim().is_empty() => Ok(token),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let token = format!("nbc_{}", Uuid::new_v4().simple());
            save_token(&token)?;
            Ok(token)
        }
        Err(error) => Err(AgentError::new("mcp_keyring_error", error.to_string())),
    }
}

pub(crate) fn save_token(token: &str) -> Result<(), AgentError> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .and_then(|entry| entry.set_password(token))
        .map_err(|error| AgentError::new("mcp_keyring_error", error.to_string()))
}

pub(crate) fn rotate_token() -> Result<String, AgentError> {
    let token = format!("nbc_{}", Uuid::new_v4().simple());
    save_token(&token)?;
    Ok(token)
}

pub(crate) fn bearer_matches(expected: &str, header_value: Option<&str>) -> bool {
    let Some(value) = header_value else {
        return false;
    };
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or(value)
        .trim();
    !expected.is_empty() && token == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn config_round_trip_preserves_fields() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("nbc-mcp-config-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        let config = McpConfig {
            enabled: true,
            port: 4010,
            allow_writes: false,
        };
        save_config(&dir, &config).unwrap();
        assert_eq!(load_config(&dir).unwrap(), config);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_input_rejects_invalid_port() {
        let error = apply_input(
            McpConfig::default(),
            McpConfigInput {
                enabled: None,
                port: Some(0),
                allow_writes: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
    }

    #[test]
    fn bearer_matches_accepts_bearer_prefix() {
        assert!(bearer_matches("secret", Some("Bearer secret")));
        assert!(bearer_matches("secret", Some("bearer secret")));
        assert!(!bearer_matches("secret", Some("Bearer other")));
        assert!(!bearer_matches("secret", None));
    }
}
