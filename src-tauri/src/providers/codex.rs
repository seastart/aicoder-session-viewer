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
/// - Session 文件: ~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl
///
/// JSONL 每行顶层结构: { timestamp, type, payload }
/// - type = "session_meta"  → payload 含 id, cwd 等元信息
/// - type = "event_msg"     → payload.type 区分 user_message / task_started 等
/// - type = "response_item" → payload.type 区分 message / function_call / function_call_output
/// - type = "turn_context"  → 包含 model、cwd 等上下文，可提取 project_path
pub struct CodexProvider {
    base_dir: PathBuf,
}

impl CodexProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        let base_dir = home.join(".codex");
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}",
                base_dir.display()
            )));
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

    /// 快速扫描 JSONL 提取摘要信息（标题、消息数、项目路径）
    fn scan_summary(content: &str) -> (Option<String>, usize, Option<String>) {
        let mut title: Option<String> = None;
        let mut project_path: Option<String> = None;
        let mut msg_count = 0;

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
            let payload = match entry.get("payload") {
                Some(p) => p,
                None => continue,
            };

            match entry_type {
                "session_meta" => {
                    // 从 session_meta 提取 cwd 作为 project_path
                    if project_path.is_none() {
                        project_path = payload
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "event_msg" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if payload_type == "user_message" {
                        msg_count += 1;
                        // 用第一条用户消息作为标题
                        if title.is_none() {
                            if let Some(msg) = payload.get("message").and_then(|m| m.as_str()) {
                                // 截取前 80 个字符，避免标题过长
                                let truncated = truncate_title(msg, 80);
                                title = Some(truncated);
                            }
                        }
                    }
                }
                "response_item" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
                    // 统计助手消息和工具调用
                    if role == "assistant"
                        || payload_type == "function_call"
                        || payload_type == "function_call_output"
                    {
                        msg_count += 1;
                    }
                }
                _ => {}
            }
        }

        (title, msg_count, project_path)
    }

    /// 解析 Codex JSONL 文件，返回消息列表、标题、项目路径
    fn parse_session_file(
        &self,
        path: &PathBuf,
    ) -> AppResult<(Vec<Message>, Option<String>, Option<String>)> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();
        let mut title: Option<String> = None;
        let mut project_path: Option<String> = None;
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
            let timestamp = Self::parse_timestamp(&entry);

            let payload = match entry.get("payload") {
                Some(p) => p,
                None => continue,
            };

            match entry_type {
                "session_meta" => {
                    if project_path.is_none() {
                        project_path = payload
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }

                "event_msg" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    if payload_type == "user_message" {
                        let text = payload
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();

                        // 用第一条用户消息作为标题
                        if title.is_none() && !text.is_empty() {
                            title = Some(truncate_title(&text, 80));
                        }

                        if !text.is_empty() {
                            messages.push(Message {
                                id: format!("codex-{}", msg_index),
                                role: Role::User,
                                content: vec![ContentBlock::Text { text }],
                                timestamp,
                                model: None,
                                usage: None,
                            });
                            msg_index += 1;
                        }
                    }
                    // task_started / turn_aborted / token_count 等事件暂不展示
                }

                "response_item" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match payload_type {
                        // 普通消息（assistant / developer / user 上下文注入）
                        "message" => {
                            let role_str =
                                payload.get("role").and_then(|r| r.as_str()).unwrap_or("");

                            // 只展示 assistant 消息，developer/user 上下文注入跳过
                            if role_str == "assistant" {
                                let blocks = Self::parse_content_blocks(payload);
                                if !blocks.is_empty() {
                                    messages.push(Message {
                                        id: format!("codex-{}", msg_index),
                                        role: Role::Assistant,
                                        content: blocks,
                                        timestamp,
                                        model: None,
                                        usage: None,
                                    });
                                    msg_index += 1;
                                }
                            }
                        }

                        // 工具调用
                        "function_call" => {
                            let tool_name = payload
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            // arguments 是 JSON 字符串，解析为 Value
                            let input = payload
                                .get("arguments")
                                .and_then(|a| {
                                    if let Some(s) = a.as_str() {
                                        serde_json::from_str(s).ok()
                                    } else {
                                        Some(a.clone())
                                    }
                                })
                                .unwrap_or(serde_json::Value::Null);

                            let tool_id = payload
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(Message {
                                id: format!("codex-{}", msg_index),
                                role: Role::Assistant,
                                content: vec![ContentBlock::ToolUse {
                                    tool_name,
                                    tool_id,
                                    input,
                                    agent_id: None,
                                }],
                                timestamp,
                                model: None,
                                usage: None,
                            });
                            msg_index += 1;
                        }

                        // 工具调用结果
                        "function_call_output" => {
                            let output = payload
                                .get("output")
                                .map(|o| {
                                    if o.is_string() {
                                        o.as_str().unwrap_or("").to_string()
                                    } else {
                                        serde_json::to_string_pretty(o).unwrap_or_default()
                                    }
                                })
                                .unwrap_or_default();

                            let tool_id = payload
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(Message {
                                id: format!("codex-{}", msg_index),
                                role: Role::Tool,
                                content: vec![ContentBlock::ToolResult {
                                    tool_id,
                                    content: output,
                                    is_error: false,
                                }],
                                timestamp,
                                model: None,
                                usage: None,
                            });
                            msg_index += 1;
                        }

                        _ => {}
                    }
                }

                // turn_context 可提取 model 信息，暂不处理
                _ => {}
            }
        }

        Ok((messages, title, project_path))
    }

    /// 从 payload.content 数组中提取 ContentBlock 列表
    fn parse_content_blocks(payload: &serde_json::Value) -> Vec<ContentBlock> {
        let mut blocks = Vec::new();

        if let Some(content_arr) = payload.get("content").and_then(|c| c.as_array()) {
            for block in content_arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "output_text" | "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        blocks
    }

    /// 解析顶层时间戳（顶层 timestamp 字段，RFC3339 格式）
    fn parse_timestamp(entry: &serde_json::Value) -> Option<DateTime<Utc>> {
        entry.get("timestamp").and_then(|t| {
            if let Some(s) = t.as_str() {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            } else if let Some(n) = t.as_f64() {
                DateTime::from_timestamp(n as i64, 0)
            } else if let Some(n) = t.as_i64() {
                DateTime::from_timestamp(n, 0)
            } else {
                None
            }
        })
    }
}

/// 截取标题，按字符边界截断，避免中间截断多字节字符
fn truncate_title(s: &str, max_chars: usize) -> String {
    // 去除换行，只取第一行
    let first_line = s.lines().next().unwrap_or(s).trim();
    if first_line.chars().count() <= max_chars {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(max_chars).collect();
        format!("{}…", truncated)
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

            let (title, msg_count, project_path) = match fs::read_to_string(path) {
                Ok(content) => Self::scan_summary(&content),
                Err(_) => (None, 0, None),
            };

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
                project_path,
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

        let (messages, title, project_path) = self.parse_session_file(path)?;
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
            project_path,
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
