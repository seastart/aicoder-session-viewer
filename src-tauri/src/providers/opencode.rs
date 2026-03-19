use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;

/// OpenCode 数据源
///
/// 存储结构：~/.local/share/opencode/opencode.db (SQLite)
/// 包含 session / message / part 三张表
pub struct OpenCodeProvider {
    db_path: PathBuf,
}

impl OpenCodeProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir().ok_or_else(|| AppError::Provider("无法获取 home 目录".into()))?;
        let db_path = home.join(".local/share/opencode/opencode.db");
        if !db_path.exists() {
            return Err(AppError::Provider("OpenCode 数据库不存在".into()));
        }
        Ok(Self { db_path })
    }

    /// 以只读模式打开数据库连接
    fn open_db(&self) -> AppResult<Connection> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(conn)
    }

    /// 解析 part 内容为 ContentBlock
    fn parse_part(part_type: &str, content: &str) -> Option<ContentBlock> {
        match part_type {
            "text" => Some(ContentBlock::Text {
                text: content.to_string(),
            }),
            "tool-invocation" | "tool_use" => {
                // 尝试解析 JSON
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(content) {
                    let tool_name = val
                        .get("toolName")
                        .or_else(|| val.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = val
                        .get("args")
                        .or_else(|| val.get("input"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    Some(ContentBlock::ToolUse {
                        tool_name,
                        tool_id: val
                            .get("toolCallId")
                            .and_then(|i| i.as_str())
                            .map(|s| s.to_string()),
                        input,
                    })
                } else {
                    Some(ContentBlock::Text {
                        text: content.to_string(),
                    })
                }
            }
            "tool-result" | "tool_result" => {
                let (result_text, is_error) =
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(content) {
                        let text = val
                            .get("result")
                            .or_else(|| val.get("content"))
                            .map(|r| {
                                if r.is_string() {
                                    r.as_str().unwrap_or("").to_string()
                                } else {
                                    serde_json::to_string_pretty(r).unwrap_or_default()
                                }
                            })
                            .unwrap_or_else(|| content.to_string());
                        let is_err = val
                            .get("isError")
                            .and_then(|e| e.as_bool())
                            .unwrap_or(false);
                        (text, is_err)
                    } else {
                        (content.to_string(), false)
                    };

                Some(ContentBlock::ToolResult {
                    tool_id: None,
                    content: result_text,
                    is_error,
                })
            }
            "reasoning" | "thinking" => Some(ContentBlock::Thinking {
                text: content.to_string(),
            }),
            _ => Some(ContentBlock::Text {
                text: content.to_string(),
            }),
        }
    }
}

impl SessionProvider for OpenCodeProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::OpenCode
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let conn = self.open_db()?;

        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.project_path, s.created_at,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) as msg_count
             FROM session s
             ORDER BY s.created_at DESC",
        )?;

        let summaries = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let project_path: Option<String> = row.get(2)?;
                let created_at: Option<String> = row.get(3)?;
                let msg_count: usize = row.get(4)?;

                let started_at = created_at.and_then(|s| {
                    // 尝试多种时间格式
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .map(|ndt| ndt.and_utc())
                        })
                });

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path,
                    started_at,
                    message_count: msg_count,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        let conn = self.open_db()?;

        // 获取 session 信息
        let summary = conn.query_row(
            "SELECT id, title, project_path, created_at,
                    (SELECT COUNT(*) FROM message WHERE session_id = ?1) as msg_count
             FROM session WHERE id = ?1",
            [session_id],
            |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let project_path: Option<String> = row.get(2)?;
                let created_at: Option<String> = row.get(3)?;
                let msg_count: usize = row.get(4)?;

                let started_at = created_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .map(|ndt| ndt.and_utc())
                        })
                });

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path,
                    started_at,
                    message_count: msg_count,
                })
            },
        )?;

        // 获取所有消息
        let mut msg_stmt = conn.prepare(
            "SELECT id, role, created_at, model FROM message
             WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;

        let mut part_stmt = conn.prepare(
            "SELECT type, content FROM part WHERE message_id = ?1 ORDER BY rowid ASC",
        )?;

        let messages: Vec<Message> = msg_stmt
            .query_map([session_id], |row| {
                let msg_id: String = row.get(0)?;
                let role_str: String = row.get(1)?;
                let created_at: Option<String> = row.get(2)?;
                let model: Option<String> = row.get(3)?;
                Ok((msg_id, role_str, created_at, model))
            })?
            .filter_map(|r| r.ok())
            .map(|(msg_id, role_str, created_at, model)| {
                let role = match role_str.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    "tool" => Role::Tool,
                    _ => Role::User,
                };

                // 获取该消息的所有 parts
                let content: Vec<ContentBlock> = part_stmt
                    .query_map([&msg_id], |row| {
                        let part_type: String = row.get(0)?;
                        let content: String = row.get(1)?;
                        Ok((part_type, content))
                    })
                    .ok()
                    .map(|rows| {
                        rows.filter_map(|r| r.ok())
                            .filter_map(|(t, c)| Self::parse_part(&t, &c))
                            .collect()
                    })
                    .unwrap_or_default();

                let timestamp = created_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .map(|ndt| ndt.and_utc())
                        })
                });

                Message {
                    id: msg_id,
                    role,
                    content,
                    timestamp,
                    model,
                    usage: None,
                }
            })
            .collect();

        Ok(Session { summary, messages })
    }

    fn search_sessions(&self, query: &str) -> AppResult<Vec<SessionSummary>> {
        let conn = self.open_db()?;
        let pattern = format!("%{}%", query);

        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.project_path, s.created_at,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) as msg_count
             FROM session s
             WHERE s.title LIKE ?1 OR s.project_path LIKE ?1
             ORDER BY s.created_at DESC",
        )?;

        let summaries = stmt
            .query_map([&pattern], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let project_path: Option<String> = row.get(2)?;
                let created_at: Option<String> = row.get(3)?;
                let msg_count: usize = row.get(4)?;

                let started_at = created_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .map(|ndt| ndt.and_utc())
                        })
                });

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path,
                    started_at,
                    message_count: msg_count,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }
}
