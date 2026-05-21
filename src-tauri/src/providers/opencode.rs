use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;

/// OpenCode 数据源
///
/// 存储结构：~/.local/share/opencode/opencode.db (SQLite)
///
/// 数据库 schema:
/// - session: id, title, directory, time_created (Unix ms), time_updated
/// - message: id, session_id, data (JSON: {role, time, modelID, providerID, ...})
/// - part: id, message_id, data (JSON: {type, text, ...})
pub struct OpenCodeProvider {
    pub(crate) db_path: PathBuf,
}

impl OpenCodeProvider {
    /// 返回默认的 opencode.db 路径
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".local/share/opencode/opencode.db"))
    }

    /// 创建 provider；`path_override` 为 None 时走默认 db 路径
    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let db_path = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        if !db_path.exists() {
            return Err(AppError::Provider(format!(
                "database not found: {}",
                db_path.display()
            )));
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

    /// 将 Unix 毫秒时间戳转为 DateTime<Utc>
    fn millis_to_datetime(ms: i64) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(ms).unwrap_or_default()
    }

    /// 解析 part 的 data JSON 为 ContentBlock
    fn parse_part_data(data_str: &str) -> Option<ContentBlock> {
        let val: serde_json::Value = serde_json::from_str(data_str).ok()?;
        let part_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match part_type {
            "text" => {
                let text = val.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Text {
                    text: text.to_string(),
                })
            }
            "tool-invocation" | "tool_use" => {
                let tool_name = val
                    .get("toolName")
                    .or_else(|| val.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let tool_id = val
                    .get("toolCallId")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let input = val
                    .get("args")
                    .or_else(|| val.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(ContentBlock::ToolUse {
                    tool_name,
                    tool_id,
                    input,
                    agent_id: None,
                })
            }
            "tool-result" | "tool_result" => {
                let result_text = val
                    .get("result")
                    .or_else(|| val.get("content"))
                    .map(|r| {
                        if r.is_string() {
                            r.as_str().unwrap_or("").to_string()
                        } else {
                            serde_json::to_string_pretty(r).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();
                let is_error = val
                    .get("isError")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);
                let tool_id = val
                    .get("toolCallId")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                Some(ContentBlock::ToolResult {
                    tool_id,
                    content: result_text,
                    is_error,
                })
            }
            "reasoning" | "thinking" => {
                let text = val.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Thinking {
                    text: text.to_string(),
                })
            }
            // 用户上传的附件（图片/文件），仅展示图片类型，PDF 等其他类型跳过
            "file" => {
                let mime = val.get("mime").and_then(|m| m.as_str()).unwrap_or("");
                if !mime.starts_with("image/") {
                    return None;
                }
                let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("");
                if url.is_empty() {
                    return None;
                }
                // 拆出 data URI 的 base64 部分，前端渲染与导出统一
                let (source, media_type) = if let Some(rest) = url.strip_prefix("data:") {
                    if let Some((meta, data)) = rest.split_once(',') {
                        let mt = meta.split(';').next().map(|s| s.to_string());
                        (data.to_string(), mt)
                    } else {
                        (url.to_string(), Some(mime.to_string()))
                    }
                } else {
                    (url.to_string(), Some(mime.to_string()))
                };
                Some(ContentBlock::Image { source, media_type })
            }
            // step-start 等控制类型跳过
            "step-start" | "step-finish" => None,
            _ => {
                // 未知类型，如果有 text 字段就当文本处理
                let text = val.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Text {
                    text: text.to_string(),
                })
            }
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
            "SELECT s.id, s.title, s.directory, s.time_created,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) as msg_count,
                    (SELECT MAX(m.time_created) FROM message m WHERE m.session_id = s.id) as last_msg_time
             FROM session s
             ORDER BY COALESCE(last_msg_time, s.time_created) DESC",
        )?;

        let summaries = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let directory: Option<String> = row.get(2)?;
                let time_created: i64 = row.get(3)?;
                let msg_count: usize = row.get(4)?;
                let last_msg_time: Option<i64> = row.get(5)?;

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path: directory,
                    started_at: Some(Self::millis_to_datetime(time_created)),
                    updated_at: last_msg_time.map(Self::millis_to_datetime),
                    message_count: msg_count,
                    total_tokens: None,
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
            "SELECT id, title, directory, time_created,
                    (SELECT COUNT(*) FROM message WHERE session_id = ?1) as msg_count,
                    (SELECT MAX(time_created) FROM message WHERE session_id = ?1) as last_msg_time
             FROM session WHERE id = ?1",
            [session_id],
            |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let directory: Option<String> = row.get(2)?;
                let time_created: i64 = row.get(3)?;
                let msg_count: usize = row.get(4)?;
                let last_msg_time: Option<i64> = row.get(5)?;

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path: directory,
                    started_at: Some(Self::millis_to_datetime(time_created)),
                    updated_at: last_msg_time.map(Self::millis_to_datetime),
                    message_count: msg_count,
                    total_tokens: None,
                })
            },
        )?;

        // 获取所有消息，从 data JSON 中提取 role/model/时间
        let mut msg_stmt = conn.prepare(
            "SELECT id, data, time_created FROM message
             WHERE session_id = ?1 ORDER BY time_created ASC",
        )?;

        // 预查询所有 part，按 message_id 分组（避免 N+1 查询）
        let mut part_stmt = conn.prepare(
            "SELECT message_id, data FROM part
             WHERE session_id = ?1 ORDER BY time_created ASC",
        )?;

        // 收集所有 part 按 message_id 分组
        let mut parts_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let part_rows = part_stmt.query_map([session_id], |row| {
            let msg_id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((msg_id, data))
        })?;
        for row in part_rows.flatten() {
            parts_map.entry(row.0).or_default().push(row.1);
        }

        let messages: Vec<Message> = msg_stmt
            .query_map([session_id], |row| {
                let msg_id: String = row.get(0)?;
                let data_str: String = row.get(1)?;
                let time_created: i64 = row.get(2)?;
                Ok((msg_id, data_str, time_created))
            })?
            .filter_map(|r| r.ok())
            .map(|(msg_id, data_str, time_created)| {
                // 从 message.data JSON 中提取 role 和 model
                let msg_data: serde_json::Value =
                    serde_json::from_str(&data_str).unwrap_or_default();
                let role_str = msg_data
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                let role = match role_str {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    "tool" => Role::Tool,
                    _ => Role::User,
                };
                let model = msg_data
                    .get("modelID")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());

                // 获取该消息的所有 parts
                let content: Vec<ContentBlock> = parts_map
                    .get(&msg_id)
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|data| Self::parse_part_data(data))
                            .collect()
                    })
                    .unwrap_or_default();

                Message {
                    id: msg_id,
                    role,
                    content,
                    timestamp: Some(Self::millis_to_datetime(time_created)),
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
            "SELECT s.id, s.title, s.directory, s.time_created,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) as msg_count,
                    (SELECT MAX(m.time_created) FROM message m WHERE m.session_id = s.id) as last_msg_time
             FROM session s
             WHERE s.title LIKE ?1 OR s.directory LIKE ?1
             ORDER BY COALESCE(last_msg_time, s.time_created) DESC",
        )?;

        let summaries = stmt
            .query_map([&pattern], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let directory: Option<String> = row.get(2)?;
                let time_created: i64 = row.get(3)?;
                let msg_count: usize = row.get(4)?;
                let last_msg_time: Option<i64> = row.get(5)?;

                Ok(SessionSummary {
                    id,
                    tool: ToolKind::OpenCode,
                    title: if title.is_empty() {
                        "OpenCode Session".to_string()
                    } else {
                        title
                    },
                    project_path: directory,
                    started_at: Some(Self::millis_to_datetime(time_created)),
                    updated_at: last_msg_time.map(Self::millis_to_datetime),
                    message_count: msg_count,
                    total_tokens: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_points_to_opencode_db() {
        let p = OpenCodeProvider::default_path().unwrap();
        assert!(p.ends_with("opencode.db"));
    }

    #[test]
    fn new_with_override_uses_passed_path() {
        // 使用 Cargo.toml 这种项目内一定存在的文件作为占位
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let p = OpenCodeProvider::new(Some(manifest.clone())).unwrap();
        assert_eq!(p.db_path, manifest);
    }

    #[test]
    fn new_with_nonexistent_path_fails() {
        let bogus = std::path::PathBuf::from("/nonexistent/path/xyz123");
        assert!(OpenCodeProvider::new(Some(bogus)).is_err());
    }
}
