use std::fs;
use std::io::{BufRead, BufReader};
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
    /// 快速扫描 JSONL 文件提取摘要（使用 BufReader 逐行读取，避免一次性加载大文件）
    ///
    /// 优化策略：先用字符串包含检查做粗筛，只对可能有用的行做 JSON 解析
    fn scan_summary(path: &PathBuf) -> (Option<String>, usize, Option<String>, Option<u64>) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return (None, 0, None, None),
        };
        let reader = BufReader::new(file);

        let mut title: Option<String> = None;
        let mut project_path: Option<String> = None;
        let mut msg_count = 0;
        let mut total_tokens: Option<u64> = None;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            // 粗筛：仅对包含关键标记的行做完整 JSON 解析
            let dominated_by_event = line.contains("\"event_msg\"");
            let dominated_by_response = line.contains("\"response_item\"");
            let dominated_by_meta = line.contains("\"session_meta\"");

            if !dominated_by_event && !dominated_by_response && !dominated_by_meta {
                continue;
            }

            let entry: serde_json::Value = match serde_json::from_str(&line) {
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
                    if project_path.is_none() {
                        project_path = payload
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "event_msg" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match payload_type {
                        "user_message" => {
                            msg_count += 1;
                            if title.is_none() {
                                if let Some(msg) =
                                    payload.get("message").and_then(|m| m.as_str())
                                {
                                    title = Some(truncate_title(msg, 80));
                                }
                            }
                        }
                        "error" => {
                            msg_count += 1;
                        }
                        "token_count" => {
                            if let Some(total) = payload
                                .get("info")
                                .and_then(|i| i.get("total_token_usage"))
                                .and_then(|t| t.get("total_tokens"))
                                .and_then(|v| v.as_u64())
                            {
                                total_tokens = Some(total);
                            }
                        }
                        _ => {}
                    }
                }
                "response_item" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
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

        (title, msg_count, project_path, total_tokens)
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

                    match payload_type {
                        "user_message" => {
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

                        // 错误消息（限流、服务异常等）→ 作为 System 消息展示
                        "error" => {
                            let text = payload
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown error")
                                .to_string();

                            messages.push(Message {
                                id: format!("codex-{}", msg_index),
                                role: Role::System,
                                content: vec![ContentBlock::Text { text }],
                                timestamp,
                                model: None,
                                usage: None,
                            });
                            msg_index += 1;
                        }

                        // token_count 事件 → 将 last_token_usage 回填到前一条 assistant 消息
                        "token_count" => {
                            if let Some(last_usage) = payload
                                .get("info")
                                .and_then(|i| i.get("last_token_usage"))
                            {
                                // Codex 的 input_tokens 已包含缓存，无需额外加算
                                let usage = TokenUsage {
                                    input_tokens: last_usage
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64()),
                                    output_tokens: last_usage
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64()),
                                    cache_read_tokens: last_usage
                                        .get("cached_input_tokens")
                                        .and_then(|v| v.as_u64()),
                                    cache_creation_tokens: None,
                                };
                                // 回填到最近一条 assistant 消息（token_count 紧跟在响应之后）
                                if let Some(last_msg) = messages
                                    .iter_mut()
                                    .rev()
                                    .find(|m| m.role == Role::Assistant && m.usage.is_none())
                                {
                                    last_msg.usage = Some(usage);
                                }
                            }
                        }

                        // task_started / turn_aborted 等事件暂不展示
                        _ => {}
                    }
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

    /// 从 state SQLite 数据库快速读取 session 列表
    ///
    /// Codex 自身维护了 `~/.codex/state_5.sqlite` 的 `threads` 表，包含
    /// title、cwd、created_at、updated_at、tokens_used 等字段，
    /// 查询毫秒级完成，比逐个扫描 JSONL 文件（166MB+）快几个数量级。
    /// 从 state SQLite 数据库快速读取 session 列表
    ///
    /// Codex 自身维护了 `~/.codex/state_5.sqlite` 的 `threads` 表，包含
    /// title、cwd、created_at、updated_at、tokens_used、rollout_path 等字段，
    /// 查询毫秒级完成，比逐个扫描 JSONL 文件（166MB+）快几个数量级。
    fn list_sessions_from_db(&self) -> AppResult<Vec<SessionSummary>> {
        let db_path = self.base_dir.join("state_5.sqlite");
        if !db_path.exists() {
            return Err(AppError::Provider("Codex state DB not found".into()));
        }

        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| AppError::Provider(format!("open Codex DB: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, title, cwd, created_at, updated_at, tokens_used, rollout_path
                 FROM threads
                 WHERE archived = 0
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| AppError::Provider(format!("prepare: {}", e)))?;

        // Step 1: 从 DB 读取所有行（毫秒级，不做文件 IO）
        struct DbRow {
            session_id: String,
            title: String,
            cwd: String,
            created_at: i64,
            updated_at: i64,
            tokens_used: i64,
        }

        let rows: Vec<DbRow> = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let rollout_path: String = row.get(6)?;
                let session_id = std::path::Path::new(&rollout_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(id);
                Ok(DbRow {
                    session_id,
                    title: row.get(1)?,
                    cwd: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    tokens_used: row.get(5)?,
                })
            })
            .map_err(|e| AppError::Provider(format!("query: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        // Step 2: 直接组装结果，不做文件 IO（消息数在 get_session 时再精确计算）
        let summaries = rows
            .into_iter()
            .map(|row| SessionSummary {
                id: row.session_id,
                tool: ToolKind::Codex,
                title: truncate_title(&row.title, 80),
                project_path: Some(row.cwd),
                started_at: chrono::DateTime::from_timestamp(row.created_at, 0),
                updated_at: chrono::DateTime::from_timestamp(row.updated_at, 0),
                message_count: 0,
                total_tokens: if row.tokens_used > 0 {
                    Some(row.tokens_used as u64)
                } else {
                    None
                },
            })
            .collect();

        Ok(summaries)
    }

    /// 回退方案：从 JSONL 文件列表扫描（SQLite 不可用时）
    fn list_sessions_from_files(&self) -> AppResult<Vec<SessionSummary>> {
        let files = self.find_session_files();
        let mut summaries = Vec::new();

        for path in &files {
            let id = Self::session_id_from_path(path);
            let date = Self::date_from_path(path);
            let (title, msg_count, project_path, total_tokens) = Self::scan_summary(path);

            let file_mtime = fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| DateTime::<Utc>::from(t));

            let started_at = date.or(file_mtime);
            let updated_at = file_mtime;

            summaries.push(SessionSummary {
                id,
                tool: ToolKind::Codex,
                title: title.unwrap_or_else(|| "Codex Session".to_string()),
                project_path,
                started_at,
                updated_at,
                message_count: msg_count,
                total_tokens,
            });
        }

        summaries.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.started_at);
            let b_time = b.updated_at.or(b.started_at);
            b_time.cmp(&a_time)
        });
        Ok(summaries)
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
        // 优先从 SQLite 读取（毫秒级），仅在 SQLite 不可用时回退到扫描 JSONL
        match self.list_sessions_from_db() {
            Ok(summaries) => {
                eprintln!("[Codex] DB 查询成功，返回 {} 条 session", summaries.len());
                return Ok(summaries);
            }
            Err(e) => {
                eprintln!("[Codex] DB 查询失败，回退到文件扫描: {}", e);
            }
        }
        self.list_sessions_from_files()
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
        let updated_at = messages.last().and_then(|m| m.timestamp);

        let total_tokens = sum_message_tokens(&messages);
        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::Codex,
            title: title.unwrap_or_else(|| "Codex Session".to_string()),
            project_path,
            started_at,
            updated_at,
            message_count: messages.len(),
            total_tokens,
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
