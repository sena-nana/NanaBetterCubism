use crate::agent::plan::{PendingPlanApproval, PlanApprovalAction, PlanDocument};
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
    pub attachments: Vec<crate::agent::images::ChatImageAttachment>,
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
pub struct PendingQuestion {
    pub action_id: String,
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
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryViewLayer {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryViewRecord {
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub title: String,
    pub layers: Vec<MemoryViewLayer>,
    pub enabled: bool,
    pub source_conversation_id: Option<String>,
    pub updated_at: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigView {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub has_api_key: bool,
    #[serde(default)]
    pub image_input_supported: Option<bool>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInput {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInternal {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_input_tokens: Option<u32>,
}

#[cfg(test)]
struct MemoryUpsertInput {
    id: Option<String>,
    scope: String,
    kind: String,
    project_id: Option<String>,
    title: String,
    body: String,
    enabled: Option<bool>,
    source_conversation_id: Option<String>,
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
              attachments_json TEXT NOT NULL DEFAULT '[]',
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS plans (
              conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
              steps_json TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pending_user_actions (
              action_id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              kind TEXT NOT NULL CHECK (kind = 'question'),
              question TEXT NOT NULL,
              options_json TEXT NOT NULL,
              tool_call_id TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pending_plan_approvals (
              action_id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL UNIQUE REFERENCES conversations(id) ON DELETE CASCADE,
              plan_json TEXT NOT NULL,
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
              updated_at TEXT NOT NULL,
              revision INTEGER NOT NULL DEFAULT 1
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
        ensure_column(
            &conn,
            "messages",
            "attachments_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_column(&conn, "llm_config", "context_window", "INTEGER")?;
        ensure_column(&conn, "llm_config", "max_input_tokens", "INTEGER")?;
        ensure_column(
            &conn,
            "conversations",
            "psd_documents_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        migrate_pending_asks(&conn)?;
        migrate_memory_bodies(&conn)?;
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS projects_document_key_unique ON projects(document_key) WHERE document_key IS NOT NULL",
        )?;
        *self.conn.lock().unwrap() = Some(conn);
        *self.path.lock().unwrap() = Some(path);
        Ok(())
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, AgentError>,
    ) -> Result<T, AgentError> {
        let guard = self.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AgentError::new("store_not_ready", "本地存储尚未初始化。"))?;
        f(conn)
    }

    pub fn data_dir(&self) -> Option<PathBuf> {
        self.path
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|path| path.parent().map(PathBuf::from))
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
    ) -> Result<bool, AgentError> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE conversations SET title = ?1 WHERE id = ?2 AND title = '新对话'",
                params![title, conversation_id],
            )?;
            Ok(updated > 0)
        })
    }

    pub fn set_conversation_title(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE conversations SET title = ?1 WHERE id = ?2",
                params![title, conversation_id],
            )?;
            if updated == 0 {
                return Err(AgentError::new("not_found", "对话不存在。"));
            }
            Ok(())
        })
    }

    pub fn list_psd_documents(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<crate::agent::psd::ChatPsdDocument>, AgentError> {
        self.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT psd_documents_json FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?;
            let mut documents: Vec<crate::agent::psd::ChatPsdDocument> =
                serde_json::from_str(json.as_deref().unwrap_or("[]")).unwrap_or_default();
            for document in &mut documents {
                document.available = PathBuf::from(&document.path).is_file();
            }
            Ok(documents)
        })
    }

    pub fn upsert_psd_document(
        &self,
        conversation_id: &str,
        document: &crate::agent::psd::ChatPsdDocument,
    ) -> Result<Vec<crate::agent::psd::ChatPsdDocument>, AgentError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let json: String = transaction
                .query_row(
                    "SELECT psd_documents_json FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| AgentError::new("not_found", "对话不存在。"))?;
            let mut documents: Vec<crate::agent::psd::ChatPsdDocument> =
                serde_json::from_str(&json).unwrap_or_default();
            if let Some(existing) = documents.iter_mut().find(|item| item.id == document.id) {
                *existing = document.clone();
            } else {
                documents.push(document.clone());
            }
            let serialized = serde_json::to_string(&documents)?;
            transaction.execute(
                "UPDATE conversations SET psd_documents_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![serialized, Utc::now().to_rfc3339(), conversation_id],
            )?;
            transaction.commit()?;
            Ok(documents)
        })
    }

    pub fn remove_psd_document(
        &self,
        conversation_id: &str,
        psd_id: &str,
    ) -> Result<Vec<crate::agent::psd::ChatPsdDocument>, AgentError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let json: String = transaction
                .query_row(
                    "SELECT psd_documents_json FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| AgentError::new("not_found", "对话不存在。"))?;
            let mut documents: Vec<crate::agent::psd::ChatPsdDocument> =
                serde_json::from_str(&json).unwrap_or_default();
            documents.retain(|item| item.id != psd_id);
            let serialized = serde_json::to_string(&documents)?;
            transaction.execute(
                "UPDATE conversations SET psd_documents_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![serialized, Utc::now().to_rfc3339(), conversation_id],
            )?;
            transaction.commit()?;
            Ok(documents)
        })
    }

    pub fn get_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, role, content, tool_name, tool_status, attachments_json, created_at
                FROM messages
                WHERE conversation_id = ?1
                ORDER BY created_at ASC, rowid ASC
                "#,
            )?;
            let rows = stmt.query_map(params![conversation_id], |row| {
                let attachments_json: String = row.get(5)?;
                let mut attachments: Vec<crate::agent::images::ChatImageAttachment> =
                    serde_json::from_str(&attachments_json).unwrap_or_default();
                for attachment in &mut attachments {
                    attachment.available = PathBuf::from(&attachment.path).is_file();
                }
                Ok(ChatMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    tool_name: row.get(3)?,
                    tool_status: row.get(4)?,
                    attachments,
                    created_at: row.get(6)?,
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
        self.append_message_with_attachments(
            conversation_id,
            role,
            content,
            tool_name,
            tool_status,
            &[],
        )
    }

    pub fn append_message_with_attachments(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        tool_name: Option<&str>,
        tool_status: Option<&str>,
        attachments: &[crate::agent::images::ChatImageAttachment],
    ) -> Result<ChatMessage, AgentError> {
        let message = ChatMessage {
            id: new_id(),
            role: role.into(),
            content: content.into(),
            tool_name: tool_name.map(str::to_string),
            tool_status: tool_status.map(str::to_string),
            attachments: attachments.to_vec(),
            created_at: Utc::now().to_rfc3339(),
        };
        let attachments_json = serde_json::to_string(&message.attachments)?;
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO messages (
                  id, conversation_id, role, content, tool_name, tool_status, attachments_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    message.id,
                    conversation_id,
                    message.role,
                    message.content,
                    message.tool_name,
                    message.tool_status,
                    attachments_json,
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

    pub fn set_pending_question(
        &self,
        question: &PendingQuestion,
        tool_call_id: &str,
    ) -> Result<(), AgentError> {
        let options_json = serde_json::to_string(&question.options)?;
        let created_at = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM pending_plan_approvals WHERE conversation_id = ?1",
                params![question.conversation_id],
            )?;
            conn.execute(
                "DELETE FROM pending_user_actions WHERE conversation_id = ?1",
                params![question.conversation_id],
            )?;
            conn.execute(
                r#"
                INSERT INTO pending_user_actions (action_id, conversation_id, kind, question, options_json, tool_call_id, created_at)
                VALUES (?1, ?2, 'question', ?3, ?4, ?5, ?6)
                "#,
                params![
                    question.action_id,
                    question.conversation_id,
                    question.question,
                    options_json,
                    tool_call_id,
                    created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn set_pending_plan_approval(
        &self,
        approval: &PendingPlanApproval,
    ) -> Result<(), AgentError> {
        let plan_json = serde_json::to_string(&approval.plan)?;
        let created_at = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            transaction.execute(
                "DELETE FROM pending_user_actions WHERE conversation_id = ?1",
                params![approval.action.conversation_id],
            )?;
            transaction.execute(
                "DELETE FROM pending_plan_approvals WHERE conversation_id = ?1",
                params![approval.action.conversation_id],
            )?;
            transaction.execute(
                r#"
                INSERT INTO pending_plan_approvals (action_id, conversation_id, plan_json, created_at)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    approval.action.action_id,
                    approval.action.conversation_id,
                    plan_json,
                    created_at,
                ],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn get_pending_plan_approval(
        &self,
        conversation_id: &str,
    ) -> Result<Option<PendingPlanApproval>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT action_id, plan_json FROM pending_plan_approvals WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            row.map(|(action_id, plan_json)| {
                let plan: PlanDocument = serde_json::from_str(&plan_json)?;
                Ok(PendingPlanApproval {
                    action: PlanApprovalAction {
                        action_id,
                        conversation_id: conversation_id.into(),
                        title: plan.title.clone(),
                    },
                    plan,
                })
            })
            .transpose()
        })
    }

    pub fn get_pending_plan_approval_by_action(
        &self,
        action_id: &str,
    ) -> Result<Option<PendingPlanApproval>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT conversation_id, plan_json FROM pending_plan_approvals WHERE action_id = ?1",
                    params![action_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            row.map(|(conversation_id, plan_json)| {
                let plan: PlanDocument = serde_json::from_str(&plan_json)?;
                Ok(PendingPlanApproval {
                    action: PlanApprovalAction {
                        action_id: action_id.into(),
                        conversation_id,
                        title: plan.title.clone(),
                    },
                    plan,
                })
            })
            .transpose()
        })
    }

    pub fn take_pending_plan_approval(
        &self,
        action_id: &str,
    ) -> Result<Option<PendingPlanApproval>, AgentError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let row = transaction
                .query_row(
                    "SELECT conversation_id, plan_json FROM pending_plan_approvals WHERE action_id = ?1",
                    params![action_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((conversation_id, plan_json)) = row else {
                return Ok(None);
            };
            transaction.execute(
                "DELETE FROM pending_plan_approvals WHERE action_id = ?1",
                params![action_id],
            )?;
            transaction.commit()?;
            let plan: PlanDocument = serde_json::from_str(&plan_json)?;
            Ok(Some(PendingPlanApproval {
                action: PlanApprovalAction {
                    action_id: action_id.into(),
                    conversation_id,
                    title: plan.title.clone(),
                },
                plan,
            }))
        })
    }

    pub fn get_pending_question(
        &self,
        conversation_id: &str,
    ) -> Result<Option<PendingQuestion>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT action_id, question, options_json FROM pending_user_actions WHERE conversation_id = ?1 AND kind = 'question'",
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
            Ok(row.map(|(action_id, question, options_json)| PendingQuestion {
                action_id,
                conversation_id: conversation_id.into(),
                question,
                options: serde_json::from_str(&options_json).unwrap_or_default(),
            }))
        })
    }

    pub fn take_pending_question(
        &self,
        action_id: &str,
    ) -> Result<Option<(PendingQuestion, String)>, AgentError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT action_id, conversation_id, question, options_json, tool_call_id FROM pending_user_actions WHERE action_id = ?1 AND kind = 'question'",
                    params![action_id],
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
            if let Some((action_id, conversation_id, question, options_json, tool_call_id)) = row {
                conn.execute(
                    "DELETE FROM pending_user_actions WHERE action_id = ?1",
                    params![action_id],
                )?;
                Ok(Some((
                    PendingQuestion {
                        action_id,
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

    pub fn clear_pending_user_action(&self, conversation_id: &str) -> Result<bool, AgentError> {
        self.with_conn(|conn| {
            let questions = conn.execute(
                "DELETE FROM pending_user_actions WHERE conversation_id = ?1",
                params![conversation_id],
            )?;
            let plans = conn.execute(
                "DELETE FROM pending_plan_approvals WHERE conversation_id = ?1",
                params![conversation_id],
            )?;
            Ok(questions > 0 || plans > 0)
        })
    }

    pub fn clear_unresumable_pending_user_actions(&self) -> Result<(), AgentError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM pending_user_actions", [])?;
            Ok(())
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
                       m.source_conversation_id, m.updated_at, m.revision
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

    pub fn list_memory_views(
        &self,
        scope: &str,
        project_id: Option<String>,
    ) -> Result<Vec<MemoryViewRecord>, AgentError> {
        crate::agent::memory_markdown::layers_for_scope(scope)?;
        self.list_memories(project_id)?
            .into_iter()
            .filter(|memory| memory.scope == scope)
            .map(memory_view)
            .collect()
    }

    fn active_memories_for_project(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>, AgentError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT m.id, m.scope, m.kind, m.project_id, p.name, m.title, m.body, m.enabled,
                       m.source_conversation_id, m.updated_at, m.revision
                FROM memories m
                LEFT JOIN projects p ON p.id = m.project_id
                WHERE m.enabled = 1 AND (
                    (m.scope = 'global' AND m.kind = 'experience')
                    OR (
                        ?1 IS NOT NULL
                        AND m.scope = 'project'
                        AND m.kind = 'stage'
                        AND m.project_id = ?1
                    )
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

    pub fn recall_agent_memories(
        &self,
        conversation_id: &str,
        request: crate::agent::memory_recall::MemoryRecallRequest,
    ) -> Result<crate::agent::memory_recall::MemoryRecallResult, AgentError> {
        let project_id = self.conversation_project_id(conversation_id)?;
        crate::agent::memory_recall::recall_memories(
            self.active_memories_for_project(project_id.as_deref())?
                .into_iter()
                .map(Into::into)
                .collect(),
            request,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_agent_memory(
        &self,
        conversation_id: &str,
        id: Option<String>,
        expected_revision: Option<i64>,
        scope: &str,
        title: &str,
        body: Option<&str>,
        layer: Option<&str>,
        content: Option<&str>,
    ) -> Result<MemoryRecord, AgentError> {
        if title.trim().is_empty() {
            return Err(AgentError::new("invalid_memory", "记忆标题不能为空。"));
        }
        match (&id, expected_revision) {
            (_, Some(revision)) if revision < 1 => {
                return Err(AgentError::new(
                    "invalid_arguments",
                    "expectedRevision 必须是正整数。",
                ));
            }
            (Some(_), None) => {
                return Err(AgentError::new(
                    "invalid_arguments",
                    "更新已有记忆时必须提供 expectedRevision。",
                ));
            }
            (None, Some(_)) => {
                return Err(AgentError::new(
                    "invalid_arguments",
                    "创建记忆时不能提供 expectedRevision。",
                ));
            }
            _ => {}
        }

        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let project_id = conversation_project_id_from(&transaction, conversation_id)?;
            let (kind, memory_project_id) = match scope {
                "project" if project_id.is_none() => {
                    return Err(AgentError::new(
                        "project_required",
                        "当前对话未归入已保存的 Cubism 项目。",
                    ));
                }
                "project" => ("stage", project_id.clone()),
                "global" => ("experience", None),
                _ => return Err(AgentError::new("invalid_memory", "记忆范围无效。")),
            };
            let existing = if let Some(id) = id.as_deref() {
                let memory = active_memory_by_id(&transaction, project_id.as_deref(), id)?;
                if memory.scope != scope {
                    return Err(AgentError::new(
                        "memory_scope_mismatch",
                        "不能更改现有记忆的范围。",
                    ));
                }
                if Some(memory.revision) != expected_revision {
                    return Err(memory_conflict());
                }
                Some(memory)
            } else {
                None
            };
            let normalized_body = match (body, layer, content) {
                (Some(body), None, None) => {
                    crate::agent::memory_markdown::validate_and_normalize(scope, title, body)?
                }
                (None, Some(layer), Some(content)) => crate::agent::memory_markdown::patch_layer(
                    scope,
                    title,
                    existing.as_ref().map(|memory| memory.body.as_str()),
                    layer,
                    content,
                )?,
                _ => {
                    return Err(AgentError::new(
                        "invalid_memory",
                        "保存记忆时请提供完整 body，或同时提供 layer 与 content。",
                    ));
                }
            };
            let memory_id = id.unwrap_or_else(new_id);
            let updated_at = Utc::now().to_rfc3339();
            if let Some(existing) = existing {
                let changed = transaction.execute(
                    r#"
                    UPDATE memories
                    SET kind = ?1, project_id = ?2, title = ?3, body = ?4, enabled = 1,
                        source_conversation_id = ?5, updated_at = ?6, revision = revision + 1
                    WHERE id = ?7 AND revision = ?8
                    "#,
                    params![
                        kind,
                        memory_project_id,
                        title.trim(),
                        normalized_body,
                        conversation_id,
                        updated_at,
                        memory_id,
                        existing.revision,
                    ],
                )?;
                if changed != 1 {
                    return Err(memory_conflict());
                }
            } else {
                transaction.execute(
                    r#"
                    INSERT INTO memories (
                      id, scope, kind, project_id, title, body, enabled,
                      source_conversation_id, updated_at, revision
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, 1)
                    "#,
                    params![
                        memory_id,
                        scope,
                        kind,
                        memory_project_id,
                        title.trim(),
                        normalized_body,
                        conversation_id,
                        updated_at,
                    ],
                )?;
            }
            let memory = memory_by_id(&transaction, &memory_id)?;
            transaction.commit()?;
            Ok(memory)
        })
    }

    pub fn archive_agent_memory(&self, conversation_id: &str, id: &str) -> Result<(), AgentError> {
        let project_id = self.conversation_project_id(conversation_id)?;
        self.require_agent_memory(project_id.as_deref(), id)?;
        self.set_memory_enabled(id, false)
    }

    fn require_agent_memory(
        &self,
        project_id: Option<&str>,
        id: &str,
    ) -> Result<MemoryRecord, AgentError> {
        self.active_memories_for_project(project_id)?
            .into_iter()
            .find(|memory| memory.id == id)
            .ok_or_else(|| {
                AgentError::new("memory_not_found", "记忆不存在、已停用或不属于当前项目。")
            })
    }

    #[cfg(test)]
    fn upsert_memory(&self, mut input: MemoryUpsertInput) -> Result<MemoryRecord, AgentError> {
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
                  updated_at = excluded.updated_at,
                  revision = memories.revision + 1
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
            let changed = conn.execute(
                "UPDATE memories SET enabled = ?1, updated_at = ?2, revision = revision + 1 WHERE id = ?3",
                params![enabled as i64, Utc::now().to_rfc3339(), id],
            )?;
            if changed == 0 {
                return Err(AgentError::new("memory_not_found", "记忆不存在。"));
            }
            Ok(())
        })
    }

    pub fn get_llm_config(&self) -> Result<LlmConfigInternal, AgentError> {
        let (base_url, model, context_window, max_input_tokens) = self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT base_url, model, context_window, max_input_tokens FROM llm_config WHERE id = 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                        ))
                    },
                )
                .optional()?;
            Ok(row.unwrap_or((None, None, None, None)))
        })?;
        Ok(LlmConfigInternal {
            base_url,
            model,
            api_key: load_api_key(),
            context_window: context_window.map(|value| value as u32),
            max_input_tokens: max_input_tokens.map(|value| value as u32),
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
            image_input_supported: None,
            context_window: config.context_window,
            max_input_tokens: config.max_input_tokens,
        })
    }

    pub fn set_llm_config(&self, input: LlmConfigInput) -> Result<LlmConfigView, AgentError> {
        let context_window = input.context_window.map(|value| value as i64);
        let max_input_tokens = input.max_input_tokens.map(|value| value as i64);
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO llm_config (id, base_url, model, context_window, max_input_tokens)
                VALUES (1, ?1, ?2, ?3, ?4)
                ON CONFLICT(id) DO UPDATE SET
                  base_url = excluded.base_url,
                  model = excluded.model,
                  context_window = excluded.context_window,
                  max_input_tokens = excluded.max_input_tokens
                "#,
                params![input.base_url, input.model, context_window, max_input_tokens],
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

fn conversation_project_id_from(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<String>, AgentError> {
    conn.query_row(
        "SELECT project_id FROM conversations WHERE id = ?1 AND archived = 0",
        params![conversation_id],
        |row| row.get(0),
    )
    .map_err(|_| AgentError::new("not_found", "对话不存在。"))
}

fn active_memory_by_id(
    conn: &Connection,
    project_id: Option<&str>,
    id: &str,
) -> Result<MemoryRecord, AgentError> {
    conn.query_row(
        r#"
        SELECT m.id, m.scope, m.kind, m.project_id, p.name, m.title, m.body, m.enabled,
               m.source_conversation_id, m.updated_at, m.revision
        FROM memories m
        LEFT JOIN projects p ON p.id = m.project_id
        WHERE m.id = ?2 AND m.enabled = 1 AND (
          (m.scope = 'global' AND m.kind = 'experience')
          OR (
            ?1 IS NOT NULL AND m.scope = 'project' AND m.kind = 'stage' AND m.project_id = ?1
          )
        )
        "#,
        params![project_id, id],
        map_memory,
    )
    .optional()?
    .ok_or_else(|| AgentError::new("memory_not_found", "记忆不存在、已停用或不属于当前项目。"))
}

fn memory_by_id(conn: &Connection, id: &str) -> Result<MemoryRecord, AgentError> {
    conn.query_row(
        r#"
        SELECT m.id, m.scope, m.kind, m.project_id, p.name, m.title, m.body, m.enabled,
               m.source_conversation_id, m.updated_at, m.revision
        FROM memories m
        LEFT JOIN projects p ON p.id = m.project_id
        WHERE m.id = ?1
        "#,
        params![id],
        map_memory,
    )
    .optional()?
    .ok_or_else(|| AgentError::new("store_error", "记忆写入后读取失败。"))
}

fn memory_conflict() -> AgentError {
    AgentError::new(
        "memory_conflict",
        "记忆已被其他对话更新，请重新召回后再保存。",
    )
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
        revision: row.get(10)?,
    })
}

fn memory_view(memory: MemoryRecord) -> Result<MemoryViewRecord, AgentError> {
    let layers = crate::agent::memory_markdown::layers_for_display(&memory.scope, &memory.body)?
        .into_iter()
        .map(|(name, content)| MemoryViewLayer { name, content })
        .collect();
    Ok(MemoryViewRecord {
        id: memory.id,
        scope: memory.scope,
        project_id: memory.project_id,
        project_name: memory.project_name,
        title: memory.title,
        layers,
        enabled: memory.enabled,
        source_conversation_id: memory.source_conversation_id,
        updated_at: memory.updated_at,
        revision: memory.revision,
    })
}

fn migrate_pending_asks(conn: &Connection) -> Result<(), AgentError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'pending_asks')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        conn.execute_batch(
            r#"
            INSERT OR IGNORE INTO pending_user_actions (
              action_id, conversation_id, kind, question, options_json, tool_call_id, created_at
            )
            SELECT ask_id, conversation_id, 'question', question, options_json, tool_call_id, created_at
            FROM pending_asks;
            DROP TABLE pending_asks;
            "#,
        )?;
    }
    Ok(())
}

fn migrate_memory_bodies(conn: &Connection) -> Result<(), AgentError> {
    let transaction = conn.unchecked_transaction()?;
    ensure_column(
        &transaction,
        "memories",
        "revision",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    let memories = {
        let mut statement = transaction.prepare("SELECT id, scope, title, body FROM memories")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (id, scope, title, body) in memories {
        let normalized =
            crate::agent::memory_markdown::normalize_for_migration(&scope, &title, &body)?;
        if normalized != body {
            transaction.execute(
                "UPDATE memories SET body = ?1, revision = revision + 1 WHERE id = ?2",
                params![normalized, id],
            )?;
        }
    }
    transaction.commit()?;
    Ok(())
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
    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    ))?;
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
    use crate::agent::memory_recall::{MemoryRecallDepth, MemoryRecallRequest, MemoryRecallScope};
    use crate::agent::plan::PlanDocument;

    fn project_body(overview: &str) -> String {
        format!("## Overview\n{overview}\n")
    }

    fn global_body(summary: &str) -> String {
        format!("## Summary\n{summary}\n")
    }

    fn recall_request(query: &str, depth: MemoryRecallDepth) -> MemoryRecallRequest {
        MemoryRecallRequest {
            query: query.into(),
            depth,
            scope: MemoryRecallScope::All,
            limit: None,
        }
    }

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
                body: project_body("已创建眼睛参数"),
                enabled: Some(true),
                source_conversation_id: Some(conversation.id.clone()),
            })
            .unwrap();
        let recall = store
            .recall_agent_memories(
                &conversation.id,
                recall_request("眼睛参数", MemoryRecallDepth::Index),
            )
            .unwrap();
        assert_eq!(recall.matches.len(), 1);
        assert_eq!(recall.matches[0].layers[0].content, "已创建眼睛参数");
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
    fn plan_approval_survives_restart_and_can_only_be_taken_once() {
        let dir = std::env::temp_dir().join(format!("nbc-plan-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.db");
        let store = AgentStore::default();
        store.open(path.clone()).unwrap();
        let conversation = store
            .create_conversation(Some("计划".into()), None)
            .unwrap();
        let pending = PendingPlanApproval {
            action: PlanApprovalAction {
                action_id: new_id(),
                conversation_id: conversation.id.clone(),
                title: "参数计划".into(),
            },
            plan: PlanDocument {
                title: "参数计划".into(),
                summary: "核对后调整".into(),
                steps: vec!["读取参数".into()],
                diagram: "flowchart TD\nA --> B".into(),
                acceptance: vec!["回读一致".into()],
                assumptions: vec!["模型已打开".into()],
                risks: vec!["版本差异".into()],
            },
        };
        store.set_pending_plan_approval(&pending).unwrap();
        drop(store);

        let reopened = AgentStore::default();
        reopened.open(path).unwrap();
        reopened.clear_unresumable_pending_user_actions().unwrap();
        assert_eq!(
            reopened
                .get_pending_plan_approval(&conversation.id)
                .unwrap()
                .unwrap()
                .plan,
            pending.plan
        );
        assert!(reopened
            .take_pending_plan_approval(&pending.action.action_id)
            .unwrap()
            .is_some());
        assert!(reopened
            .take_pending_plan_approval(&pending.action.action_id)
            .unwrap()
            .is_none());
        drop(reopened);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn memory_views_filter_scope_and_project_with_ordered_layers() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        let project_a = store
            .create_conversation(
                Some("项目 A".into()),
                Some(("c:/models/a.cmo3", "C:/models/A.cmo3")),
            )
            .unwrap();
        let project_b = store
            .create_conversation(
                Some("项目 B".into()),
                Some(("c:/models/b.cmo3", "C:/models/B.cmo3")),
            )
            .unwrap();

        store
            .upsert_agent_memory(
                &project_a.id,
                None,
                None,
                "project",
                "A 阶段",
                Some("## Overview\nA 摘要\n## Stage\nA 阶段内容"),
                None,
                None,
            )
            .unwrap();
        store
            .upsert_agent_memory(
                &project_b.id,
                None,
                None,
                "project",
                "B 阶段",
                Some("## Overview\nB 摘要"),
                None,
                None,
            )
            .unwrap();
        let legacy = store
            .upsert_memory(MemoryUpsertInput {
                id: None,
                scope: "global".into(),
                kind: "experience".into(),
                project_id: None,
                title: "旧经验".into(),
                body: "旧版纯文本经验".into(),
                enabled: Some(false),
                source_conversation_id: None,
            })
            .unwrap();

        let project_views = store
            .list_memory_views("project", project_a.project_id)
            .unwrap();
        assert_eq!(project_views.len(), 1);
        assert_eq!(project_views[0].title, "A 阶段");
        assert_eq!(
            project_views[0]
                .layers
                .iter()
                .map(|layer| layer.name.as_str())
                .collect::<Vec<_>>(),
            crate::agent::memory_markdown::PROJECT_LAYERS
        );
        assert_eq!(project_views[0].layers[1].content, "A 阶段内容");
        assert!(project_views[0].layers[2].content.is_empty());

        let global_views = store.list_memory_views("global", None).unwrap();
        assert_eq!(global_views.len(), 1);
        assert_eq!(global_views[0].id, legacy.id);
        assert!(!global_views[0].enabled);
        assert_eq!(global_views[0].layers[0].content, "旧版纯文本经验");
        assert!(global_views[0]
            .layers
            .iter()
            .skip(1)
            .all(|layer| layer.content.is_empty()));

        assert_eq!(
            store
                .set_memory_enabled("missing-memory", true)
                .unwrap_err()
                .code,
            "memory_not_found"
        );
    }

    #[test]
    fn agent_memory_access_is_active_and_scoped_to_the_conversation_project() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        let project_a = store
            .create_conversation(
                Some("项目 A".into()),
                Some(("c:/models/a.cmo3", "C:/models/A.cmo3")),
            )
            .unwrap();
        let project_b = store
            .create_conversation(
                Some("项目 B".into()),
                Some(("c:/models/b.cmo3", "C:/models/B.cmo3")),
            )
            .unwrap();
        let inbox = store
            .create_conversation(Some("收集箱".into()), None)
            .unwrap();

        let stage_a = store
            .upsert_agent_memory(
                &project_a.id,
                None,
                None,
                "project",
                "A 阶段",
                Some(&project_body("A 已完成")),
                None,
                None,
            )
            .unwrap();
        let stage_b = store
            .upsert_agent_memory(
                &project_b.id,
                None,
                None,
                "project",
                "B 阶段",
                Some(&project_body("B 已完成")),
                None,
                None,
            )
            .unwrap();
        let experience = store
            .upsert_agent_memory(
                &project_a.id,
                None,
                None,
                "global",
                "通用经验",
                Some(&global_body("可跨项目复用")),
                None,
                None,
            )
            .unwrap();
        let archived = store
            .upsert_agent_memory(
                &project_a.id,
                None,
                None,
                "project",
                "旧阶段",
                Some(&project_body("已过期")),
                None,
                None,
            )
            .unwrap();
        store
            .archive_agent_memory(&project_a.id, &archived.id)
            .unwrap();

        let project_recall = store
            .recall_agent_memories(
                &project_a.id,
                recall_request("完成 复用", MemoryRecallDepth::Focused),
            )
            .unwrap();
        assert_eq!(project_recall.matches.len(), 2);
        assert_eq!(project_recall.matches[0].id, stage_a.id);
        assert!(project_recall
            .matches
            .iter()
            .any(|memory| memory.id == experience.id && memory.scope == "global"));
        assert!(!project_recall
            .matches
            .iter()
            .any(|memory| memory.id == stage_b.id || memory.id == archived.id));

        let updated_stage = store
            .upsert_agent_memory(
                &project_a.id,
                Some(stage_a.id.clone()),
                Some(stage_a.revision),
                "project",
                "A 阶段",
                Some(&project_body("A 已完成并验证")),
                None,
                None,
            )
            .unwrap();
        assert_eq!(updated_stage.id, stage_a.id);
        assert!(updated_stage.body.contains("A 已完成并验证"));

        let patched = store
            .upsert_agent_memory(
                &project_a.id,
                Some(stage_a.id.clone()),
                Some(updated_stage.revision),
                "project",
                "A 阶段",
                None,
                Some("Stage"),
                Some("ParamAngleX 已对齐。"),
            )
            .unwrap();
        assert!(patched.body.contains("## Stage\nParamAngleX 已对齐。"));
        assert!(patched.body.contains("## Overview\nA 已完成并验证"));
        let structure_update = store
            .upsert_agent_memory(
                &project_a.id,
                Some(stage_a.id.clone()),
                Some(patched.revision),
                "project",
                "A 阶段",
                None,
                Some("Structure"),
                Some("参数位于 Face 组。"),
            )
            .unwrap();
        assert!(matches!(
            store.upsert_agent_memory(
                &project_a.id,
                Some(stage_a.id.clone()),
                Some(patched.revision),
                "project",
                "A 阶段",
                None,
                Some("Decisions"),
                Some("保留标准参数 ID。"),
            ),
            Err(error) if error.code == "memory_conflict"
        ));
        let reconciled = store
            .upsert_agent_memory(
                &project_a.id,
                Some(stage_a.id.clone()),
                Some(structure_update.revision),
                "project",
                "A 阶段",
                None,
                Some("Decisions"),
                Some("保留标准参数 ID。"),
            )
            .unwrap();
        assert!(reconciled.body.contains("## Structure\n参数位于 Face 组。"));
        assert!(reconciled.body.contains("## Decisions\n保留标准参数 ID。"));
        let layered = store
            .recall_agent_memories(
                &project_a.id,
                recall_request("ParamAngleX", MemoryRecallDepth::Focused),
            )
            .unwrap();
        assert_eq!(layered.matches[0].id, stage_a.id);
        assert_eq!(layered.matches[0].layers[1].name, "Stage");

        assert_eq!(
            store
                .upsert_agent_memory(
                    &project_a.id,
                    None,
                    None,
                    "project",
                    "非法",
                    Some("纯文本无分层"),
                    None,
                    None,
                )
                .unwrap_err()
                .code,
            "invalid_memory_body"
        );

        let inbox_recall = store
            .recall_agent_memories(
                &inbox.id,
                recall_request("复用", MemoryRecallDepth::Focused),
            )
            .unwrap();
        assert_eq!(
            inbox_recall
                .matches
                .iter()
                .map(|memory| memory.id.as_str())
                .collect::<Vec<_>>(),
            vec![experience.id.as_str()]
        );

        assert!(matches!(
            store.upsert_agent_memory(
                &project_b.id,
                Some(stage_a.id.clone()),
                Some(stage_a.revision),
                "project",
                "越权更新",
                Some(&project_body("不应成功")),
                None,
                None,
            ),
            Err(error) if error.code == "memory_not_found"
        ));
        assert!(matches!(
            store.archive_agent_memory(&project_b.id, &stage_a.id),
            Err(error) if error.code == "memory_not_found"
        ));
        assert!(matches!(
            store.upsert_agent_memory(
                &project_a.id,
                Some(experience.id),
                Some(experience.revision),
                "project",
                "错误改类",
                Some(&project_body("不应成功")),
                None,
                None,
            ),
            Err(error) if error.code == "memory_scope_mismatch"
        ));
        assert!(matches!(
            store.upsert_agent_memory(
                &inbox.id,
                None,
                None,
                "project",
                "无项目",
                Some(&project_body("不应成功")),
                None,
                None,
            ),
            Err(error) if error.code == "project_required"
        ));
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
            .set_pending_question(
                &PendingQuestion {
                    action_id: "ask-1".into(),
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
                    "pending_user_actions",
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
                CREATE TABLE pending_asks (
                  ask_id TEXT PRIMARY KEY,
                  conversation_id TEXT NOT NULL,
                  question TEXT NOT NULL,
                  options_json TEXT NOT NULL,
                  tool_call_id TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );
                INSERT INTO pending_asks (
                  ask_id, conversation_id, question, options_json, tool_call_id, created_at
                ) VALUES (
                  'legacy-question', 'older', '继续？', '["继续"]', 'tool-call', '2026-01-01T00:00:00Z'
                );
                CREATE TABLE memories (
                  id TEXT PRIMARY KEY,
                  scope TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  project_id TEXT,
                  title TEXT NOT NULL,
                  body TEXT NOT NULL,
                  enabled INTEGER NOT NULL DEFAULT 1,
                  source_conversation_id TEXT,
                  updated_at TEXT NOT NULL
                );
                INSERT INTO memories (
                  id, scope, kind, project_id, title, body, enabled,
                  source_conversation_id, updated_at
                ) VALUES
                  (
                    'legacy-stage', 'project', 'stage', 'legacy-project', '旧阶段',
                    '旧阶段正文

## 自定义标题
保留内容', 1, 'older', '2026-01-01T00:00:00Z'
                  ),
                  (
                    'legacy-global', 'global', 'experience', NULL, '旧经验',
                    '旧版全局经验', 1, 'older', '2026-01-01T00:00:00Z'
                  );
                "#,
            )
            .unwrap();
        }

        let store = AgentStore::default();
        store.open(path.clone()).unwrap();
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
        let pending = store.get_pending_question("older").unwrap().unwrap();
        assert_eq!(pending.action_id, "legacy-question");
        assert_eq!(pending.options, vec!["继续"]);
        let project_memories = store
            .list_memory_views("project", Some("legacy-project".into()))
            .unwrap();
        assert_eq!(project_memories.len(), 1);
        assert_eq!(project_memories[0].layers[0].name, "Overview");
        assert!(project_memories[0].layers[0]
            .content
            .contains("### 自定义标题\n保留内容"));
        let global_memories = store.list_memory_views("global", None).unwrap();
        assert_eq!(global_memories[0].layers[0].content, "旧版全局经验");
        let updated = store
            .upsert_agent_memory(
                "older",
                Some("legacy-stage".into()),
                Some(project_memories[0].revision),
                "project",
                "旧阶段",
                None,
                Some("Stage"),
                Some("迁移后可继续更新。"),
            )
            .unwrap();
        assert!(updated.body.contains("## Overview\n旧阶段正文"));
        assert!(updated.body.contains("## Stage\n迁移后可继续更新。"));
        let revision_after_update = updated.revision;
        let reopened = AgentStore::default();
        reopened.open(path.clone()).unwrap();
        assert_eq!(
            reopened
                .list_memory_views("project", Some("legacy-project".into()))
                .unwrap()[0]
                .revision,
            revision_after_update
        );

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

    #[test]
    fn memory_migration_rolls_back_schema_and_bodies_on_invalid_layered_data() {
        let dir = std::env::temp_dir().join(format!("nbc-memory-rollback-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE memories (
                  id TEXT PRIMARY KEY,
                  scope TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  project_id TEXT,
                  title TEXT NOT NULL,
                  body TEXT NOT NULL,
                  enabled INTEGER NOT NULL DEFAULT 1,
                  source_conversation_id TEXT,
                  updated_at TEXT NOT NULL
                );
                INSERT INTO memories (
                  id, scope, kind, project_id, title, body, enabled,
                  source_conversation_id, updated_at
                ) VALUES
                  (
                    'legacy', 'global', 'experience', NULL, '旧经验', '完整保留的旧正文',
                    1, NULL, '2026-01-01T00:00:00Z'
                  ),
                  (
                    'invalid', 'global', 'experience', NULL, '损坏分层',
                    '## Summary
第一份
## Summary
第二份', 1, NULL, '2026-01-01T00:00:00Z'
                  );
                "#,
            )
            .unwrap();
        }

        let store = AgentStore::default();
        assert!(matches!(
            store.open(path.clone()),
            Err(error) if error.code == "invalid_memory_body"
        ));
        assert!(matches!(
            store.list_memory_views("global", None),
            Err(error) if error.code == "store_not_ready"
        ));

        let conn = Connection::open(&path).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(memories)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column == "revision"));
        let legacy_body: String = conn
            .query_row("SELECT body FROM memories WHERE id = 'legacy'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(legacy_body, "完整保留的旧正文");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn set_conversation_title_overrides_non_default_title() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        let conversation = store.create_conversation(None, None).unwrap();
        assert!(store
            .set_conversation_title_if_default(&conversation.id, "临时标题")
            .unwrap());
        assert!(!store
            .set_conversation_title_if_default(&conversation.id, "不会生效")
            .unwrap());
        store
            .set_conversation_title(&conversation.id, "AI短标题")
            .unwrap();
        assert_eq!(store.list_conversations().unwrap()[0].title, "AI短标题");
    }

    #[test]
    fn image_attachments_roundtrip_and_report_missing_managed_files() {
        let dir = std::env::temp_dir().join(format!("nbc-message-images-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("image.png");
        std::fs::write(&image_path, b"image").unwrap();
        let store = AgentStore::default();
        store.open(dir.join("agent.db")).unwrap();
        let conversation = store.create_conversation(None, None).unwrap();
        let attachment = crate::agent::images::ChatImageAttachment {
            id: new_id(),
            name: "image.png".into(),
            path: image_path.to_string_lossy().into_owned(),
            mime: "image/png".into(),
            size: 5,
            available: true,
        };
        store
            .append_message_with_attachments(
                &conversation.id,
                "user",
                "查看",
                None,
                None,
                std::slice::from_ref(&attachment),
            )
            .unwrap();
        assert_eq!(
            store.get_messages(&conversation.id).unwrap()[0].attachments,
            vec![attachment.clone()]
        );

        std::fs::remove_file(image_path).unwrap();
        assert!(!store.get_messages(&conversation.id).unwrap()[0].attachments[0].available);
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn psd_documents_roundtrip_report_availability_and_remove() {
        let dir = std::env::temp_dir().join(format!("nbc-psd-docs-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let psd_path = dir.join("doc.psd");
        std::fs::write(&psd_path, b"psd").unwrap();
        let store = AgentStore::default();
        store.open(dir.join("agent.db")).unwrap();
        let conversation = store.create_conversation(None, None).unwrap();

        let document = crate::agent::psd::ChatPsdDocument {
            id: new_id(),
            name: "doc.psd".into(),
            path: psd_path.to_string_lossy().into_owned(),
            width: 1024,
            height: 768,
            color_mode: "rgb".into(),
            layer_count: 3,
            available: true,
        };
        let listed = store
            .upsert_psd_document(&conversation.id, &document)
            .unwrap();
        assert_eq!(listed, vec![document.clone()]);
        assert_eq!(
            store.list_psd_documents(&conversation.id).unwrap(),
            vec![document.clone()]
        );

        let updated = crate::agent::psd::ChatPsdDocument {
            layer_count: 5,
            ..document.clone()
        };
        let after_update = store
            .upsert_psd_document(&conversation.id, &updated)
            .unwrap();
        assert_eq!(after_update, vec![updated.clone()]);
        assert_eq!(
            store.list_psd_documents(&conversation.id).unwrap(),
            vec![updated.clone()]
        );

        std::fs::remove_file(&psd_path).unwrap();
        let reloaded = store.list_psd_documents(&conversation.id).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert!(
            !reloaded[0].available,
            "missing PSD file should be reported as unavailable"
        );

        let remaining = store
            .remove_psd_document(&conversation.id, &updated.id)
            .unwrap();
        assert!(remaining.is_empty());
        assert!(store.list_psd_documents(&conversation.id).unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn llm_config_context_window_round_trips() {
        let store = AgentStore::default();
        store.open(":memory:".into()).unwrap();
        let initial = store.get_llm_config_view().unwrap();
        assert!(initial.context_window.is_none());
        assert!(initial.max_input_tokens.is_none());

        let view = store
            .set_llm_config(LlmConfigInput {
                base_url: Some("https://example.com/v1".into()),
                api_key: None,
                model: Some("mock".into()),
                clear_api_key: false,
                context_window: Some(128000),
                max_input_tokens: Some(100000),
            })
            .unwrap();
        assert_eq!(view.context_window, Some(128000));
        assert_eq!(view.max_input_tokens, Some(100000));

        let internal = store.get_llm_config().unwrap();
        assert_eq!(internal.context_window, Some(128000));
        assert_eq!(internal.max_input_tokens, Some(100000));

        // 清空字段后持久化并重新读取
        let cleared = store
            .set_llm_config(LlmConfigInput {
                base_url: Some("https://example.com/v1".into()),
                api_key: None,
                model: Some("mock".into()),
                clear_api_key: false,
                context_window: None,
                max_input_tokens: None,
            })
            .unwrap();
        assert!(cleared.context_window.is_none());
        assert!(cleared.max_input_tokens.is_none());
        let internal_after = store.get_llm_config().unwrap();
        assert!(internal_after.context_window.is_none());
        assert!(internal_after.max_input_tokens.is_none());
    }

    #[test]
    fn llm_config_view_serializes_budget_fields_camel_case() {
        let view = LlmConfigView {
            base_url: None,
            model: None,
            has_api_key: false,
            image_input_supported: None,
            context_window: Some(64000),
            max_input_tokens: Some(50000),
        };
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["contextWindow"], 64000);
        assert_eq!(value["maxInputTokens"], 50000);
        assert!(value.get("context_window").is_none());
        assert!(value.get("max_input_tokens").is_none());
    }
}
