use crate::agent::{new_id, AgentError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

const LLM_KEYRING_ACCOUNT: &str = "openai-compatible-api-key";
const KEYRING_SERVICE: &str = "com.senanana.nanabettercubism";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub updated_at: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPlan {
    pub conversation_id: String,
    pub steps: Vec<PlanStep>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAsk {
    pub ask_id: String,
    pub conversation_id: String,
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub title: String,
    pub body: String,
    pub enabled: bool,
    pub source_conversation_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigView {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInput {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInternal {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUpsertInput {
    pub id: Option<String>,
    pub scope: String,
    pub kind: String,
    pub project_id: Option<String>,
    pub title: String,
    pub body: String,
    pub enabled: Option<bool>,
    pub source_conversation_id: Option<String>,
}

#[derive(Default)]
pub struct AgentStore {
    conn: Mutex<Option<Connection>>,
    path: Mutex<Option<PathBuf>>,
}

impl AgentStore {
    pub fn open(&self, path: PathBuf) -> Result<(), AgentError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AgentError::new("store_error", e.to_string()))?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversations (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              project_id TEXT REFERENCES projects(id),
              pinned INTEGER NOT NULL DEFAULT 0,
              archived INTEGER NOT NULL DEFAULT 0,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
              id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              role TEXT NOT NULL,
              content TEXT NOT NULL,
              tool_name TEXT,
              tool_status TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS plans (
              conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
              steps_json TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pending_asks (
              ask_id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              question TEXT NOT NULL,
              options_json TEXT NOT NULL,
              tool_call_id TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memories (
              id TEXT PRIMARY KEY,
              scope TEXT NOT NULL,
              kind TEXT NOT NULL,
              project_id TEXT REFERENCES projects(id),
              title TEXT NOT NULL,
              body TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              source_conversation_id TEXT,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS llm_config (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              base_url TEXT,
              model TEXT
            );
            CREATE TABLE IF NOT EXISTS tool_traces (
              id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              tool_call_id TEXT NOT NULL,
              tool_name TEXT NOT NULL,
              arguments_summary TEXT NOT NULL,
              result_summary TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            "#,
        )?;
        ensure_column(
            &conn,
            "conversations",
            "archived",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        *self.conn.lock().unwrap() = Some(conn);
        *self.path.lock().unwrap() = Some(path);
        Ok(())
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, AgentError>) -> Result<T, AgentError> {
        let guard = self.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AgentError::new("store_not_ready", "本地存储尚未初始化。"))?;
        f(conn)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT c.id, c.title, c.project_id, p.name, c.updated_at, c.pinned
                FROM conversations c
                LEFT JOIN projects p ON p.id = c.project_id
                WHERE c.archived = 0
                ORDER BY c.pinned DESC, c.updated_at DESC
                "#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    project_id: row.get(2)?,
                    project_name: row.get(3)?,
                    updated_at: row.get(4)?,
                    pinned: row.get::<_, i64>(5)? != 0,
                })
            })?;
            Ok(rows.filter_map(Result::ok).collect())
        })
    }

    pub fn create_conversation(&self, title: Option<String>) -> Result<ConversationSummary, AgentError> {
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        let title = title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "新对话".into());
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO conversations (id, title, project_id, pinned, archived, updated_at) VALUES (?1, ?2, NULL, 0, 0, ?3)",
                params![id, title, now],
            )?;
            Ok(())
        })?;
        Ok(ConversationSummary {
            id,
            title,
            project_id: None,
            project_name: None,
            updated_at: now,
            pinned: false,
        })
    }

    pub fn touch_conversation(&self, conversation_id: &str) -> Result<(), AgentError> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![now, conversation_id],
            )?;
            Ok(())
        })
    }

    pub fn ensure_active_conversation(&self, conversation_id: &str) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            let active = conn
                .query_row(
                    "SELECT 1 FROM conversations WHERE id = ?1 AND archived = 0",
                    params![conversation_id],
                    |_| Ok(()),
                )
                .optional()?;
            active.ok_or_else(|| AgentError::new("not_found", "对话不存在或已归档。"))
        })
    }

    pub fn set_conversation_pinned(
        &self,
        conversation_id: &str,
        pinned: bool,
    ) -> Result<bool, AgentError> {
        let changed = self.with_conn(|conn| {
            Ok(conn.execute(
                "UPDATE conversations SET pinned = ?1 WHERE id = ?2 AND archived = 0",
                params![pinned as i64, conversation_id],
            )?)
        })?;
        if changed == 0 {
            return Err(AgentError::new("not_found", "对话不存在或已归档。"));
        }
        Ok(pinned)
    }

    pub fn archive_conversation(&self, conversation_id: &str) -> Result<bool, AgentError> {
        let changed = self.with_conn(|conn| {
            Ok(conn.execute(
                "UPDATE conversations SET archived = 1, updated_at = ?1 WHERE id = ?2 AND archived = 0",
                params![Utc::now().to_rfc3339(), conversation_id],
            )?)
        })?;
        if changed == 0 {
            return Err(AgentError::new("not_found", "对话不存在或已归档。"));
        }
        Ok(true)
    }

    pub fn set_conversation_title_if_default(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE conversations SET title = ?1 WHERE id = ?2 AND title = '新对话'",
                params![title, conversation_id],
            )?;
            Ok(())
        })
    }

    pub fn get_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, role, content, tool_name, tool_status, created_at
                FROM messages
                WHERE conversation_id = ?1
                ORDER BY created_at ASC, rowid ASC
                "#,
            )?;
            let rows = stmt.query_map(params![conversation_id], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    tool_name: row.get(3)?,
                    tool_status: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            Ok(rows.filter_map(Result::ok).collect())
        })
    }

    pub fn append_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        tool_name: Option<&str>,
        tool_status: Option<&str>,
    ) -> Result<ChatMessage, AgentError> {
        let message = ChatMessage {
            id: new_id(),
            role: role.into(),
            content: content.into(),
            tool_name: tool_name.map(str::to_string),
            tool_status: tool_status.map(str::to_string),
            created_at: Utc::now().to_rfc3339(),
        };
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO messages (id, conversation_id, role, content, tool_name, tool_status, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    message.id,
                    conversation_id,
                    message.role,
                    message.content,
                    message.tool_name,
                    message.tool_status,
                    message.created_at
                ],
            )?;
            Ok(())
        })?;
        self.touch_conversation(conversation_id)?;
        Ok(message)
    }

    pub fn append_tool_trace(
        &self,
        conversation_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        result: &str,
        status: &str,
    ) -> Result<(), AgentError> {
        let id = new_id();
        let created_at = Utc::now().to_rfc3339();
        let arguments_summary = truncate_summary(arguments, 500);
        let result_summary = truncate_summary(result, 500);
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO tool_traces (
                  id, conversation_id, tool_call_id, tool_name, arguments_summary, result_summary, status, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    id,
                    conversation_id,
                    tool_call_id,
                    tool_name,
                    arguments_summary,
                    result_summary,
                    status,
                    created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_plan(&self, conversation_id: &str) -> Result<Option<ConversationPlan>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT steps_json, updated_at FROM plans WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            Ok(row.map(|(steps_json, updated_at)| ConversationPlan {
                conversation_id: conversation_id.into(),
                steps: serde_json::from_str(&steps_json).unwrap_or_default(),
                updated_at,
            }))
        })
    }

    pub fn upsert_plan(
        &self,
        conversation_id: &str,
        steps: Vec<PlanStep>,
    ) -> Result<ConversationPlan, AgentError> {
        let updated_at = Utc::now().to_rfc3339();
        let steps_json = serde_json::to_string(&steps)?;
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO plans (conversation_id, steps_json, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(conversation_id) DO UPDATE SET
                  steps_json = excluded.steps_json,
                  updated_at = excluded.updated_at
                "#,
                params![conversation_id, steps_json, updated_at],
            )?;
            Ok(())
        })?;
        Ok(ConversationPlan {
            conversation_id: conversation_id.into(),
            steps,
            updated_at,
        })
    }

    pub fn set_pending_ask(
        &self,
        ask: &PendingAsk,
        tool_call_id: &str,
    ) -> Result<(), AgentError> {
        let options_json = serde_json::to_string(&ask.options)?;
        let created_at = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM pending_asks WHERE conversation_id = ?1",
                params![ask.conversation_id],
            )?;
            conn.execute(
                r#"
                INSERT INTO pending_asks (ask_id, conversation_id, question, options_json, tool_call_id, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    ask.ask_id,
                    ask.conversation_id,
                    ask.question,
                    options_json,
                    tool_call_id,
                    created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_pending_ask(&self, conversation_id: &str) -> Result<Option<PendingAsk>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT ask_id, question, options_json FROM pending_asks WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            Ok(row.map(|(ask_id, question, options_json)| PendingAsk {
                ask_id,
                conversation_id: conversation_id.into(),
                question,
                options: serde_json::from_str(&options_json).unwrap_or_default(),
            }))
        })
    }

    pub fn take_pending_ask(
        &self,
        ask_id: &str,
    ) -> Result<Option<(PendingAsk, String)>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT ask_id, conversation_id, question, options_json, tool_call_id FROM pending_asks WHERE ask_id = ?1",
                    params![ask_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((ask_id, conversation_id, question, options_json, tool_call_id)) = row {
                conn.execute("DELETE FROM pending_asks WHERE ask_id = ?1", params![ask_id])?;
                Ok(Some((
                    PendingAsk {
                        ask_id,
                        conversation_id,
                        question,
                        options: serde_json::from_str(&options_json).unwrap_or_default(),
                    },
                    tool_call_id,
                )))
            } else {
                Ok(None)
            }
        })
    }

    pub fn clear_pending_ask(&self, conversation_id: &str) -> Result<bool, AgentError> {
        self.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM pending_asks WHERE conversation_id = ?1",
                params![conversation_id],
            )?;
            Ok(deleted > 0)
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, name, updated_at FROM projects ORDER BY updated_at DESC")?;
            let rows = stmt.query_map([], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            })?;
            Ok(rows.filter_map(Result::ok).collect())
        })
    }

    pub fn upsert_project(
        &self,
        id: Option<String>,
        name: String,
    ) -> Result<ProjectRecord, AgentError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AgentError::new("invalid_project", "项目名不能为空。"));
        }
        let now = Utc::now().to_rfc3339();
        let id = id.unwrap_or_else(new_id);
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO projects (id, name, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?3)
                ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at
                "#,
                params![id, name, now],
            )?;
            Ok(())
        })?;
        Ok(ProjectRecord {
            id,
            name,
            updated_at: now,
        })
    }

    pub fn bind_project(
        &self,
        conversation_id: &str,
        project_id: Option<String>,
    ) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE conversations SET project_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![project_id, Utc::now().to_rfc3339(), conversation_id],
            )?;
            Ok(())
        })
    }

    pub fn conversation_project_id(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>, AgentError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT project_id FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .map_err(|_| AgentError::new("not_found", "对话不存在。"))
        })
    }

    pub fn list_memories(
        &self,
        project_id: Option<String>,
    ) -> Result<Vec<MemoryRecord>, AgentError> {
        self.with_conn(|conn| {
            let mut sql = String::from(
                r#"
                SELECT m.id, m.scope, m.kind, m.project_id, p.name, m.title, m.body, m.enabled,
                       m.source_conversation_id, m.updated_at
                FROM memories m
                LEFT JOIN projects p ON p.id = m.project_id
                WHERE 1 = 1
                "#,
            );
            if project_id.is_some() {
                sql.push_str(" AND (m.scope = 'global' OR m.project_id = ?1)");
            }
            sql.push_str(" ORDER BY m.updated_at DESC");
            let mut stmt = conn.prepare(&sql)?;
            let mapped = if let Some(project_id) = project_id {
                stmt.query_map(params![project_id], map_memory)?
                    .filter_map(Result::ok)
                    .collect()
            } else {
                stmt.query_map([], map_memory)?
                    .filter_map(Result::ok)
                    .collect()
            };
            Ok(mapped)
        })
    }

    pub fn memories_for_injection(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT m.id, m.scope, m.kind, m.project_id, p.name, m.title, m.body, m.enabled,
                       m.source_conversation_id, m.updated_at
                FROM memories m
                LEFT JOIN projects p ON p.id = m.project_id
                WHERE m.enabled = 1
                  AND (
                    m.scope = 'global'
                    OR (?1 IS NOT NULL AND m.scope = 'project' AND m.project_id = ?1)
                  )
                ORDER BY m.scope DESC, m.updated_at DESC
                "#,
            )?;
            let rows = stmt
                .query_map(params![project_id], map_memory)?
                .filter_map(Result::ok)
                .collect();
            Ok(rows)
        })
    }

    pub fn upsert_memory(&self, input: MemoryUpsertInput) -> Result<MemoryRecord, AgentError> {
        let id = input.id.unwrap_or_else(new_id);
        let enabled = input.enabled.unwrap_or(true);
        let updated_at = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO memories (
                  id, scope, kind, project_id, title, body, enabled, source_conversation_id, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                  scope = excluded.scope,
                  kind = excluded.kind,
                  project_id = excluded.project_id,
                  title = excluded.title,
                  body = excluded.body,
                  enabled = excluded.enabled,
                  source_conversation_id = excluded.source_conversation_id,
                  updated_at = excluded.updated_at
                "#,
                params![
                    id,
                    input.scope,
                    input.kind,
                    input.project_id,
                    input.title,
                    input.body,
                    enabled as i64,
                    input.source_conversation_id,
                    updated_at
                ],
            )?;
            Ok(())
        })?;
        let list = self.list_memories(None)?;
        list.into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AgentError::new("store_error", "记忆写入后读取失败。"))
    }

    pub fn set_memory_enabled(&self, id: &str, enabled: bool) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE memories SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                params![enabled as i64, Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
    }

    pub fn get_llm_config(&self) -> Result<LlmConfigInternal, AgentError> {
        let (base_url, model) = self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT base_url, model FROM llm_config WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?;
            Ok(row.unwrap_or((None, None)))
        })?;
        Ok(LlmConfigInternal {
            base_url,
            model,
            api_key: load_api_key(),
        })
    }

    pub fn get_llm_config_view(&self) -> Result<LlmConfigView, AgentError> {
        let config = self.get_llm_config()?;
        Ok(LlmConfigView {
            base_url: config.base_url,
            model: config.model,
            has_api_key: config
                .api_key
                .as_ref()
                .map(|value| !value.is_empty())
                .unwrap_or(false),
        })
    }

    pub fn set_llm_config(&self, input: LlmConfigInput) -> Result<LlmConfigView, AgentError> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO llm_config (id, base_url, model)
                VALUES (1, ?1, ?2)
                ON CONFLICT(id) DO UPDATE SET base_url = excluded.base_url, model = excluded.model
                "#,
                params![input.base_url, input.model],
            )?;
            Ok(())
        })?;
        if input.clear_api_key {
            clear_api_key();
        } else if let Some(api_key) = input.api_key.filter(|value| !value.is_empty()) {
            save_api_key(&api_key);
        }
        self.get_llm_config_view()
    }

    pub fn db_path(&self) -> Option<PathBuf> {
        self.path.lock().unwrap().clone()
    }

    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.db_path()
            .and_then(|path| path.parent().map(|parent| parent.join("cache")))
    }
}

fn map_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        kind: row.get(2)?,
        project_id: row.get(3)?,
        project_name: row.get(4)?,
        title: row.get(5)?,
        body: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        source_conversation_id: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AgentError> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }
    conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))?;
    Ok(())
}

fn load_api_key() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, LLM_KEYRING_ACCOUNT)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .filter(|value| !value.is_empty())
}

fn save_api_key(api_key: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, LLM_KEYRING_ACCOUNT) {
        let _ = entry.set_password(api_key);
    }
}

fn clear_api_key() {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, LLM_KEYRING_ACCOUNT) {
        let _ = entry.delete_credential();
    }
}

pub(crate) fn truncate_summary(text: &str, max: usize) -> String {
    let trimmed = text.replace('\n', " ");
    let mut chars = trimmed.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip_conversation_and_memory() {
        let dir = std::env::temp_dir().join(format!("nbc-agent-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = AgentStore::default();
        store.open(dir.join("agent.db")).unwrap();
        let conversation = store.create_conversation(Some("测试".into())).unwrap();
        store
            .append_message(&conversation.id, "user", "hello", None, None)
            .unwrap();
        let project = store.upsert_project(None, "角色A".into()).unwrap();
        store
            .bind_project(&conversation.id, Some(project.id.clone()))
            .unwrap();
        store
            .upsert_memory(MemoryUpsertInput {
                id: None,
                scope: "project".into(),
                kind: "stage".into(),
                project_id: Some(project.id.clone()),
                title: "阶段".into(),
                body: "已创建眼睛参数".into(),
                enabled: Some(true),
                source_conversation_id: Some(conversation.id.clone()),
            })
            .unwrap();
        let memories = store.memories_for_injection(Some(&project.id)).unwrap();
        assert_eq!(memories.len(), 1);
        store
            .append_tool_trace(
                &conversation.id,
                "call_1",
                "get_editor_snapshot",
                "{}",
                "{\"ok\":true}",
                "finished",
            )
            .unwrap();
        let count: i64 = store
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM tool_traces WHERE conversation_id = ?1",
                    params![conversation.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_and_manages_active_conversations_without_deleting_history() {
        let dir = std::env::temp_dir().join(format!("nbc-agent-migration-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE conversations (
                  id TEXT PRIMARY KEY,
                  title TEXT NOT NULL,
                  project_id TEXT,
                  pinned INTEGER NOT NULL DEFAULT 0,
                  updated_at TEXT NOT NULL
                );
                INSERT INTO conversations (id, title, project_id, pinned, updated_at)
                VALUES ('older', '较早对话', NULL, 0, '2026-01-01T00:00:00Z');
                "#,
            )
            .unwrap();
        }

        let store = AgentStore::default();
        store.open(path).unwrap();
        let newer = store.create_conversation(Some("较新对话".into())).unwrap();
        store
            .append_message(&newer.id, "user", "保留的消息", None, None)
            .unwrap();
        store.set_conversation_pinned("older", true).unwrap();

        let listed = store.list_conversations().unwrap();
        assert_eq!(listed[0].id, "older");
        assert!(listed[0].pinned);

        store.archive_conversation(&newer.id).unwrap();
        assert!(store
            .list_conversations()
            .unwrap()
            .iter()
            .all(|conversation| conversation.id != newer.id));
        let messages: i64 = store
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                    params![newer.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(messages, 1);
        assert!(matches!(
            store.ensure_active_conversation(&newer.id),
            Err(error) if error.code == "not_found"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
