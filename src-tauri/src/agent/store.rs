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
              document_key TEXT,
              document_path TEXT,
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
        ensure_column(&conn, "projects", "document_key", "TEXT")?;
        ensure_column(&conn, "projects", "document_path", "TEXT")?;
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS projects_document_key_unique ON projects(document_key) WHERE document_key IS NOT NULL",
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

    pub fn create_conversation(
        &self,
        title: Option<String>,
        document: Option<(&str, &str)>,
    ) -> Result<ConversationSummary, AgentError> {
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        let title = title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "新对话".into());
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let project = document
                .map(|(key, path)| resolve_document_project(&transaction, key, path, &now))
                .transpose()?;
            transaction.execute(
                "INSERT INTO conversations (id, title, project_id, pinned, archived, updated_at) VALUES (?1, ?2, ?3, 0, 0, ?4)",
                params![id, title, project.as_ref().map(|item| item.0.as_str()), now],
            )?;
            transaction.commit()?;
            Ok(ConversationSummary {
                id,
                title,
                project_id: project.as_ref().map(|item| item.0.clone()),
                project_name: project.map(|item| item.1),
                updated_at: now,
                pinned: false,
            })
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
            active.ok_or_else(|| AgentError::new("not_found", "对话不存在。"))
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
            return Err(AgentError::new("not_found", "对话不存在。"));
        }
        Ok(pinned)
    }

    pub fn delete_conversation(&self, conversation_id: &str) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            transaction.execute(
                "UPDATE memories SET source_conversation_id = NULL WHERE source_conversation_id = ?1",
                params![conversation_id],
            )?;
            let deleted = transaction.execute(
                "DELETE FROM conversations WHERE id = ?1 AND archived = 0",
                params![conversation_id],
            )?;
            if deleted == 0 {
                return Err(AgentError::new("not_found", "对话不存在。"));
            }
            transaction.commit()?;
            Ok(())
        })
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

    pub fn upsert_memory(&self, mut input: MemoryUpsertInput) -> Result<MemoryRecord, AgentError> {
        match input.scope.as_str() {
            "project" if input.project_id.is_none() => {
                return Err(AgentError::new(
                    "project_required",
                    "当前对话未归入已保存的 Cubism 项目。",
                ));
            }
            "global" => input.project_id = None,
            "project" => {}
            _ => return Err(AgentError::new("invalid_memory", "记忆范围无效。")),
        }
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

fn resolve_document_project(
    conn: &Connection,
    document_key: &str,
    document_path: &str,
    now: &str,
) -> Result<(String, String), AgentError> {
    let existing = conn
        .query_row(
            "SELECT id FROM projects WHERE document_key = ?1",
            params![document_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let (project_id, reused) = match existing {
        Some(id) => (id, true),
        None => (new_id(), false),
    };
    if reused {
        conn.execute(
            "UPDATE projects SET document_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![document_path, now, project_id],
        )?;
    } else {
        conn.execute(
            "INSERT INTO projects (id, name, document_key, document_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![project_id, document_stem(document_path), document_key, document_path, now],
        )?;
    }
    refresh_duplicate_document_names(conn, document_path)?;
    let name = conn.query_row(
        "SELECT name FROM projects WHERE id = ?1",
        params![project_id],
        |row| row.get(0),
    )?;
    Ok((project_id, name))
}

fn refresh_duplicate_document_names(
    conn: &Connection,
    current_path: &str,
) -> Result<(), AgentError> {
    let current_stem = document_stem(current_path);
    let projects = {
        let mut stmt = conn.prepare(
            "SELECT id, document_path FROM projects WHERE document_key IS NOT NULL AND document_path IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(Result::ok)
            .filter(|(_, path)| same_document_stem(&document_stem(path), &current_stem))
            .collect::<Vec<_>>();
        rows
    };
    let duplicate = projects.len() > 1;
    for (id, path) in projects {
        let stem = document_stem(&path);
        let name = if duplicate {
            document_parent(&path)
                .map(|parent| format!("{stem} — {parent}"))
                .unwrap_or(stem)
        } else {
            stem
        };
        conn.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
    }
    Ok(())
}

fn document_stem(path: &str) -> String {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .rfind('.')
        .map(|index| &file_name[..index])
        .filter(|name| !name.is_empty())
        .unwrap_or(file_name)
        .to_string()
}

fn document_parent(path: &str) -> Option<&str> {
    path.rsplit('/').nth(1).filter(|value| !value.is_empty())
}

fn same_document_stem(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
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
        let conversation = store
            .create_conversation(
                Some("测试".into()),
                Some(("c:/models/角色a.cmo3", "C:/models/角色A.cmo3")),
            )
            .unwrap();
        let project_id = conversation.project_id.clone().unwrap();
        store
            .append_message(&conversation.id, "user", "hello", None, None)
            .unwrap();
        store
            .upsert_memory(MemoryUpsertInput {
                id: None,
                scope: "project".into(),
                kind: "stage".into(),
                project_id: Some(project_id.clone()),
                title: "阶段".into(),
                body: "已创建眼睛参数".into(),
                enabled: Some(true),
                source_conversation_id: Some(conversation.id.clone()),
            })
            .unwrap();
        let memories = store.memories_for_injection(Some(&project_id)).unwrap();
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
    fn deleting_conversation_cascades_owned_data_and_detaches_memories() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        let deleted = store
            .create_conversation(Some("待删除".into()), None)
            .unwrap();
        let retained = store
            .create_conversation(Some("保留".into()), None)
            .unwrap();
        store
            .append_message(&deleted.id, "user", "删除的消息", None, None)
            .unwrap();
        store
            .append_message(&retained.id, "user", "保留的消息", None, None)
            .unwrap();
        store
            .upsert_plan(
                &deleted.id,
                vec![PlanStep {
                    id: "step-1".into(),
                    title: "测试".into(),
                    status: "pending".into(),
                }],
            )
            .unwrap();
        store
            .set_pending_ask(
                &PendingAsk {
                    ask_id: "ask-1".into(),
                    conversation_id: deleted.id.clone(),
                    question: "继续？".into(),
                    options: Vec::new(),
                },
                "tool-call",
            )
            .unwrap();
        store
            .append_tool_trace(
                &deleted.id,
                "call-1",
                "get_editor_snapshot",
                "{}",
                "{}",
                "finished",
            )
            .unwrap();
        let memory = store
            .upsert_memory(MemoryUpsertInput {
                id: None,
                scope: "global".into(),
                kind: "experience".into(),
                project_id: None,
                title: "保留的记忆".into(),
                body: "记忆内容".into(),
                enabled: Some(true),
                source_conversation_id: Some(deleted.id.clone()),
            })
            .unwrap();

        store.delete_conversation(&deleted.id).unwrap();

        let memories = store.list_memories(None).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, memory.id);
        assert_eq!(memories[0].source_conversation_id, None);
        assert_eq!(store.get_messages(&retained.id).unwrap().len(), 1);
        store
            .with_conn(|conn| {
                for table in [
                    "conversations",
                    "messages",
                    "plans",
                    "pending_asks",
                    "tool_traces",
                ] {
                    let count: i64 = conn.query_row(
                        &format!(
                            "SELECT COUNT(*) FROM {table} WHERE {} = ?1",
                            if table == "conversations" {
                                "id"
                            } else {
                                "conversation_id"
                            }
                        ),
                        params![deleted.id],
                        |row| row.get(0),
                    )?;
                    assert_eq!(count, 0, "{table} still contains deleted conversation data");
                }
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            store.delete_conversation(&deleted.id),
            Err(error) if error.code == "not_found"
        ));
    }

    #[test]
    fn automatic_projects_reuse_paths_and_disambiguate_duplicate_file_names() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();

        let first = store
            .create_conversation(
                Some("A1".into()),
                Some((
                    "c:/characters/alpha/nana.cmo3",
                    "C:/Characters/Alpha/Nana.cmo3",
                )),
            )
            .unwrap();
        let repeated = store
            .create_conversation(
                Some("A2".into()),
                Some((
                    "c:/characters/alpha/nana.cmo3",
                    "C:/Characters/Alpha/Nana.cmo3",
                )),
            )
            .unwrap();
        let second = store
            .create_conversation(
                Some("B".into()),
                Some((
                    "d:/characters/beta/nana.cmo3",
                    "D:/Characters/Beta/Nana.cmo3",
                )),
            )
            .unwrap();

        assert_eq!(first.project_id, repeated.project_id);
        assert_ne!(first.project_id, second.project_id);
        let conversations = store.list_conversations().unwrap();
        let project_names = conversations
            .iter()
            .map(|item| item.project_name.as_deref().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            project_names,
            std::collections::BTreeSet::from(["Nana — Alpha", "Nana — Beta"])
        );
        assert_eq!(store.list_projects().unwrap().len(), 2);
    }

    #[test]
    fn automatic_project_resolution_rolls_back_when_conversation_insert_fails() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER reject_conversation BEFORE INSERT ON conversations BEGIN SELECT RAISE(ABORT, 'rejected'); END;",
                )?;
                Ok(())
            })
            .unwrap();

        let result = store.create_conversation(
            Some("失败".into()),
            Some(("c:/characters/nana.cmo3", "C:/Characters/Nana.cmo3")),
        );

        assert!(result.is_err());
        assert!(store.list_projects().unwrap().is_empty());
        assert!(store.list_conversations().unwrap().is_empty());
    }

    #[test]
    fn migrates_legacy_schema_without_purging_archived_history() {
        let dir = std::env::temp_dir().join(format!("nbc-agent-migration-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE projects (
                  id TEXT PRIMARY KEY,
                  name TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );
                INSERT INTO projects (id, name, created_at, updated_at)
                VALUES ('legacy-project', '旧项目', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                CREATE TABLE conversations (
                  id TEXT PRIMARY KEY,
                  title TEXT NOT NULL,
                  project_id TEXT,
                  pinned INTEGER NOT NULL DEFAULT 0,
                  updated_at TEXT NOT NULL
                );
                INSERT INTO conversations (id, title, project_id, pinned, updated_at)
                VALUES ('older', '较早对话', 'legacy-project', 0, '2026-01-01T00:00:00Z');
                "#,
            )
            .unwrap();
        }

        let store = AgentStore::default();
        store.open(path).unwrap();
        let newer = store
            .create_conversation(Some("较新对话".into()), None)
            .unwrap();
        store
            .append_message(&newer.id, "user", "保留的消息", None, None)
            .unwrap();
        store.set_conversation_pinned("older", true).unwrap();

        let listed = store.list_conversations().unwrap();
        assert_eq!(listed[0].id, "older");
        assert!(listed[0].pinned);
        assert_eq!(listed[0].project_name.as_deref(), Some("旧项目"));
        assert_eq!(store.list_projects().unwrap().len(), 1);

        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE conversations SET archived = 1 WHERE id = ?1",
                    params![newer.id],
                )?;
                Ok(())
            })
            .unwrap();
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
