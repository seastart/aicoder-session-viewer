use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;

/// Codex 数据源
///
/// 存储结构：
/// - 全局历史: ~/.codex/history.jsonl
/// - Session 文件: ~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl
///
/// JSONL 格式中每行可能是 session_meta 或 response_item
pub struct CodexProvider {
    base_dir: PathBuf,
}

impl CodexProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir().ok_or_else(|| AppError::Provider("无法获取 home 目录".into()))?;
        let base_dir = home.join(".codex");
        if !base_dir.exists() {
            return Err(AppError::Provider("~/.codex 目录不存在".into()));
        }
        Ok(Self { base_dir })
    }

    /// 扫描所有 rollout JSONL 文件
    fn find_session_files(&self) -> Vec<PathBuf> {
        let sessions_dir = self.base_dir.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        WalkDir::new(&sessions_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "jsonl")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"))
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    }

    /// 从文件路径提取 session id
    fn session_id_from_path(path: &PathBuf) -> String {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    /// 从路径中提取日期 ({Y}/{M}/{D})
    fn date_from_path(path: &PathBuf) -> Option<DateTime<Utc>> {
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        // 找到 "sessions" 之后的 Y/M/D
        let sessions_idx = components.iter().position(|&c| c == "sessions")?;
        if components.len() < sessions_idx + 4 {
            return None;
        }

        let year: i32 = components[sessions_idx + 1].parse().ok()?;
        let month: u32 = components[sessions_idx + 2].parse().ok()?;
        let day: u32 = components[sessions_idx + 3].parse().ok()?;

        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|ndt| ndt.and_utc())
    }

    /// 解析 Codex JSONL 文件
    fn parse_session_file(&self, path: &PathBuf) -> AppResult<(Vec<Message>, Option<String>)> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();
        let mut title = None;
        let mut msg_index = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match entry_type {
                // session 元数据
                "session_meta" | "session.meta" => {
                    if let Some(t) = entry
                        .get("title")
                        .or_else(|| entry.get("instruction"))
                        .and_then(|t| t.as_str())
                    {
                        title = Some(t.to_string());
                    }
                }
                // 用户消息
                "user_message" | "message.user" => {
                    let text = entry
                        .get("content")
                        .or_else(|| entry.get("text"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !text.is_empty() {
                        messages.push(Message {
                            id: format!("codex-{}", msg_index),
                            role: Role::User,
                            content: vec![ContentBlock::Text { text }],
                            timestamp: Self::parse_timestamp(&entry),
                            model: None,
                            usage: None,
                        });
                        msg_index += 1;
                    }
                }
                // 助手响应
                "response_item" | "message.assistant" | "response.completed" => {
                    if let Some(msg) = self.parse_response_item(&entry, msg_index) {
                        messages.push(msg);
                        msg_index += 1;
                    }
                }
                // function call 相关
                "function_call" | "tool_call" => {
                    let tool_name = entry
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = entry
                        .get("arguments")
                        .or_else(|| entry.get("input"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    messages.push(Message {
                        id: format!("codex-{}", msg_index),
                        role: Role::Assistant,
                        content: vec![ContentBlock::ToolUse {
                            tool_name,
                            tool_id: entry
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string()),
                            input,
                        }],
                        timestamp: Self::parse_timestamp(&entry),
                        model: None,
                        usage: None,
                    });
                    msg_index += 1;
                }
                "function_call_output" | "tool_result" => {
                    let output = entry
                        .get("output")
                        .or_else(|| entry.get("content"))
                        .map(|o| {
                            if o.is_string() {
                                o.as_str().unwrap_or("").to_string()
                            } else {
                                serde_json::to_string_pretty(o).unwrap_or_default()
                            }
                        })
                        .unwrap_or_default();

                    messages.push(Message {
                        id: format!("codex-{}", msg_index),
                        role: Role::Tool,
                        content: vec![ContentBlock::ToolResult {
                            tool_id: entry
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string()),
                            content: output,
                            is_error: false,
                        }],
                        timestamp: Self::parse_timestamp(&entry),
                        model: None,
                        usage: None,
                    });
                    msg_index += 1;
                }
                _ => {
                    // 其他类型尝试提取 content
                    if let Some(text) = entry.get("content").and_then(|c| c.as_str()) {
                        let role = entry
                            .get("role")
                            .and_then(|r| r.as_str())
                            .map(|r| match r {
                                "user" => Role::User,
                                "assistant" => Role::Assistant,
                                "system" => Role::System,
                                _ => Role::Assistant,
                            })
                            .unwrap_or(Role::Assistant);

                        messages.push(Message {
                            id: format!("codex-{}", msg_index),
                            role,
                            content: vec![ContentBlock::Text {
                                text: text.to_string(),
                            }],
                            timestamp: Self::parse_timestamp(&entry),
                            model: None,
                            usage: None,
                        });
                        msg_index += 1;
                    }
                }
            }
        }

        Ok((messages, title))
    }

    /// 解析 response_item 为 Message
    fn parse_response_item(
        &self,
        entry: &serde_json::Value,
        index: usize,
    ) -> Option<Message> {
        let mut content_blocks = Vec::new();

        // 尝试从 content 数组提取
        if let Some(content_arr) = entry.get("content").and_then(|c| c.as_array()) {
            for block in content_arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" | "output_text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            content_blocks.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
            content_blocks.push(ContentBlock::Text {
                text: text.to_string(),
            });
        } else if let Some(text) = entry.get("content").and_then(|c| c.as_str()) {
            content_blocks.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }

        if content_blocks.is_empty() {
            return None;
        }

        Some(Message {
            id: format!("codex-{}", index),
            role: Role::Assistant,
            content: content_blocks,
            timestamp: Self::parse_timestamp(entry),
            model: entry
                .get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string()),
            usage: None,
        })
    }

    /// 解析时间戳
    fn parse_timestamp(entry: &serde_json::Value) -> Option<DateTime<Utc>> {
        entry
            .get("timestamp")
            .or_else(|| entry.get("created_at"))
            .and_then(|t| {
                if let Some(s) = t.as_str() {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                } else if let Some(n) = t.as_f64() {
                    // Unix timestamp（秒）
                    DateTime::from_timestamp(n as i64, 0)
                } else if let Some(n) = t.as_i64() {
                    DateTime::from_timestamp(n, 0)
                } else {
                    None
                }
            })
    }
}

impl SessionProvider for CodexProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::Codex
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let files = self.find_session_files();
        let mut summaries = Vec::new();

        for path in &files {
            let id = Self::session_id_from_path(path);
            let date = Self::date_from_path(path);

            // 快速扫描获取 session 元信息
            let (msg_count, title) = match fs::read_to_string(path) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
                    let count = lines.len();
                    let title = lines.iter().find_map(|line| {
                        serde_json::from_str::<serde_json::Value>(line)
                            .ok()
                            .and_then(|v| {
                                let t = v.get("type")?.as_str()?;
                                if t == "session_meta" || t == "session.meta" {
                                    v.get("title")
                                        .or_else(|| v.get("instruction"))
                                        .and_then(|t| t.as_str())
                                        .map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                    });
                    (count, title)
                }
                Err(_) => (0, None),
            };

            // 如果没有从路径获取日期，用文件修改时间
            let started_at = date.or_else(|| {
                fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| DateTime::<Utc>::from(t))
            });

            summaries.push(SessionSummary {
                id,
                tool: ToolKind::Codex,
                title: title.unwrap_or_else(|| "Codex Session".to_string()),
                project_path: None,
                started_at,
                message_count: msg_count,
            });
        }

        summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        let files = self.find_session_files();
        let path = files
            .iter()
            .find(|p| Self::session_id_from_path(p) == session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;

        let (messages, title) = self.parse_session_file(path)?;
        let started_at = Self::date_from_path(path).or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| DateTime::<Utc>::from(t))
        });

        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::Codex,
            title: title.unwrap_or_else(|| "Codex Session".to_string()),
            project_path: None,
            started_at,
            message_count: messages.len(),
        };

        Ok(Session { summary, messages })
    }

    fn search_sessions(&self, query: &str) -> AppResult<Vec<SessionSummary>> {
        let query_lower = query.to_lowercase();
        let all = self.list_sessions()?;
        Ok(all
            .into_iter()
            .filter(|s| s.title.to_lowercase().contains(&query_lower))
            .collect())
    }
}
