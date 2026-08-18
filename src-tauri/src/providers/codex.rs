use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::{search, SessionProvider};

/// Codex 数据源
///
/// 存储结构：
/// - Session 文件: ~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl
///
/// JSONL 每行顶层结构: { timestamp, type, payload }
/// - type = "session_meta"  → payload 含 id, cwd 等元信息
/// - type = "event_msg"     → payload.type 区分 user_message / item_completed / task_started 等
/// - type = "response_item" → payload.type 区分 message / function_call / custom_tool_call 等
/// - type = "turn_context"  → 包含 model、cwd 等上下文，可提取 project_path
///
/// 版本差异（Codex CLI）：
/// - ≤0.146：用户输入记录在 `event_msg.user_message`
/// - ≥0.147：`user_message` 事件被移除，用户输入改由
///   `event_msg.item_completed` 中 `item.type == "UserMessage"` 承载；
///   `response_item(role=user)` 里虽也有文本，但混杂 AGENTS.md/环境上下文注入，不可直接采用
pub struct CodexProvider {
    base_dir: PathBuf,
}

impl CodexProvider {
    /// 返回默认的 ~/.codex 路径
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".codex"))
    }

    /// 创建 provider；`path_override` 为 None 时走默认路径
    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
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
        // 用户消息来源（新旧格式互斥，锁定先出现的那一种，避免重复计数）
        let mut user_src = UserMsgSource::Unknown;

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
                        // Codex ≤0.146 的用户消息事件
                        "user_message" => {
                            if !user_src.accept(UserMsgSource::Legacy) {
                                continue;
                            }
                            msg_count += 1;
                            if title.is_none() {
                                if let Some(msg) =
                                    payload.get("message").and_then(|m| m.as_str())
                                {
                                    title = Some(truncate_title(msg, 80));
                                }
                            }
                        }
                        // Codex ≥0.147 的统一事件流，只取其中的 UserMessage
                        "item_completed" => {
                            let item = payload.get("item");
                            let is_user_msg = item
                                .and_then(|i| i.get("type"))
                                .and_then(|t| t.as_str())
                                .is_some_and(|t| t == "UserMessage");
                            if !is_user_msg || !user_src.accept(UserMsgSource::ItemCompleted) {
                                continue;
                            }
                            msg_count += 1;
                            if title.is_none() {
                                let text = item.map(collect_item_text).unwrap_or_default();
                                if !text.is_empty() {
                                    title = Some(truncate_title(&text, 80));
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
                    if role == "assistant" || is_tool_item(payload_type) {
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
        // 缓冲：用户粘贴的图片在 response_item(role=user) 中以 input_image 出现，
        // 紧跟其后的 event_msg.user_message 才是真正展示的用户消息（含 [Image #N] 占位文本）。
        // 此处先把图片暂存，待 user_message 到达时一并附加到消息内容里。
        let mut pending_user_images: Vec<ContentBlock> = Vec::new();
        // 用户消息来源（新旧格式互斥，锁定先出现的那一种，避免同一条消息出现两次）
        let mut user_src = UserMsgSource::Unknown;

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
                        // Codex ≤0.146 的用户消息事件
                        "user_message" => {
                            if !user_src.accept(UserMsgSource::Legacy) {
                                continue;
                            }
                            let text = payload
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string();

                            push_user_message(
                                &mut messages,
                                &mut msg_index,
                                &mut title,
                                &mut pending_user_images,
                                text,
                                timestamp,
                            );
                        }

                        // Codex ≥0.147 的统一事件流：用户输入改由 item.type == "UserMessage" 承载
                        // （其余 AgentMessage / CommandExecution 等都是 response_item 的重复投影，跳过）
                        "item_completed" => {
                            let item = payload.get("item");
                            let is_user_msg = item
                                .and_then(|i| i.get("type"))
                                .and_then(|t| t.as_str())
                                .is_some_and(|t| t == "UserMessage");
                            if !is_user_msg || !user_src.accept(UserMsgSource::ItemCompleted) {
                                continue;
                            }

                            let text = item.map(collect_item_text).unwrap_or_default();
                            push_user_message(
                                &mut messages,
                                &mut msg_index,
                                &mut title,
                                &mut pending_user_images,
                                text,
                                timestamp,
                            );
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
                            } else if role_str == "user" {
                                // 用户 response_item：仅提取 input_image，文本由 user_message 事件提供
                                pending_user_images
                                    .extend(Self::parse_user_image_blocks(payload));
                            }
                            // developer 等上下文注入直接跳过
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

                        // 自定义工具调用（exec / apply_patch 等）：input 是原始字符串
                        // （JS 代码、patch 文本），不是 JSON，原样保留交给前端展示
                        "custom_tool_call" => {
                            let tool_name = payload
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let input = payload
                                .get("input")
                                .cloned()
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

                        // 联网搜索：action 里含 query/queries，作为工具调用参数展示
                        "web_search_call" => {
                            let input = payload
                                .get("action")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);

                            messages.push(Message {
                                id: format!("codex-{}", msg_index),
                                role: Role::Assistant,
                                content: vec![ContentBlock::ToolUse {
                                    tool_name: "web_search".to_string(),
                                    tool_id: payload
                                        .get("id")
                                        .and_then(|i| i.as_str())
                                        .map(|s| s.to_string()),
                                    input,
                                    agent_id: None,
                                }],
                                timestamp,
                                model: None,
                                usage: None,
                            });
                            msg_index += 1;
                        }

                        // 工具调用结果（两种 output 形态统一由 extract_tool_output 处理）
                        "function_call_output" | "custom_tool_call_output" => {
                            let output = Self::extract_tool_output(payload);

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

    /// 提取工具调用结果文本
    ///
    /// `output` 有两种形态：
    /// - ≤0.146：直接是字符串
    /// - ≥0.147：`[{type: "input_text", text: "..."}, ...]` 分段数组，需拼接
    fn extract_tool_output(payload: &serde_json::Value) -> String {
        let Some(output) = payload.get("output") else {
            return String::new();
        };

        if let Some(s) = output.as_str() {
            return s.to_string();
        }

        if let Some(arr) = output.as_array() {
            let parts: Vec<&str> = arr
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect();
            // 分段本身已自带换行，直接拼接即可还原原始输出
            if !parts.is_empty() {
                return parts.concat();
            }
        }

        serde_json::to_string_pretty(output).unwrap_or_default()
    }

    /// 从用户 response_item 的 content 数组中提取图片块
    ///
    /// Codex 把粘贴/附件图片记录为 `{type: "input_image", image_url: "data:image/png;base64,..."}`，
    /// 紧邻的 input_text 则用 `<image name=[Image #N]>` / `</image>` 作为分隔符——
    /// 这些文本会在 user_message 中以占位符形式重新出现，所以这里只关心图片本身。
    fn parse_user_image_blocks(payload: &serde_json::Value) -> Vec<ContentBlock> {
        let mut images = Vec::new();
        let Some(content_arr) = payload.get("content").and_then(|c| c.as_array()) else {
            return images;
        };

        for block in content_arr {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if block_type != "input_image" {
                continue;
            }
            let Some(url) = block.get("image_url").and_then(|u| u.as_str()) else {
                continue;
            };
            // 拆解 data URI，方便前端显示与导出统一处理
            let (source, media_type) = if let Some(rest) = url.strip_prefix("data:") {
                if let Some((meta, data)) = rest.split_once(',') {
                    let mt = meta.split(';').next().map(|s| s.to_string());
                    (data.to_string(), mt)
                } else {
                    (url.to_string(), None)
                }
            } else {
                (url.to_string(), None)
            };

            images.push(ContentBlock::Image { source, media_type });
        }
        images
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

/// 用户消息的记录来源
///
/// Codex ≤0.146 用 `event_msg.user_message`，≥0.147 改用 `event_msg.item_completed`
/// 中的 `UserMessage`。同一文件只会用其中一种；这里锁定先出现的那一种，
/// 万一将来两者并存也不会把同一条用户消息解析两次。
#[derive(PartialEq, Clone, Copy)]
enum UserMsgSource {
    Unknown,
    /// ≤0.146: event_msg.user_message
    Legacy,
    /// ≥0.147: event_msg.item_completed → item.type == "UserMessage"
    ItemCompleted,
}

impl UserMsgSource {
    /// 判断本条事件是否应被采纳；首次调用时锁定来源
    fn accept(&mut self, candidate: UserMsgSource) -> bool {
        if *self == UserMsgSource::Unknown {
            *self = candidate;
        }
        *self == candidate
    }
}

/// 判断 response_item 的 payload.type 是否为工具调用/结果（计入消息数）
fn is_tool_item(payload_type: &str) -> bool {
    matches!(
        payload_type,
        "function_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "web_search_call"
    )
}

/// 拼接 item_completed 里 item.content 数组的文本
///
/// 结构为 `[{type: "text", text: "..."}, ...]`
fn collect_item_text(item: &serde_json::Value) -> String {
    let Some(arr) = item.get("content").and_then(|c| c.as_array()) else {
        return String::new();
    };
    arr.iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 生成一条用户消息：文本 + 之前缓冲的图片；顺带用首条消息填充标题
///
/// 文本与图片任一存在即产出消息（纯图片消息也要展示）。
fn push_user_message(
    messages: &mut Vec<Message>,
    msg_index: &mut usize,
    title: &mut Option<String>,
    pending_user_images: &mut Vec<ContentBlock>,
    text: String,
    timestamp: Option<DateTime<Utc>>,
) {
    // 用第一条用户消息作为标题
    if title.is_none() && !text.is_empty() {
        *title = Some(truncate_title(&text, 80));
    }

    let has_text = !text.is_empty();
    let has_images = !pending_user_images.is_empty();
    if !has_text && !has_images {
        return;
    }

    let mut blocks: Vec<ContentBlock> = Vec::new();
    if has_text {
        blocks.push(ContentBlock::Text { text });
    }
    blocks.append(pending_user_images);

    messages.push(Message {
        id: format!("codex-{}", msg_index),
        role: Role::User,
        content: blocks,
        timestamp,
        model: None,
        usage: None,
    });
    *msg_index += 1;
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

    fn search_sessions(
        &self,
        query: &str,
        include_content: bool,
    ) -> AppResult<Vec<SessionSummary>> {
        let all = self.list_sessions()?;
        // session_id（即文件名 stem）→ 文件路径映射，供内容全文匹配使用（仅深度搜索时构建）
        let path_map: std::collections::HashMap<String, PathBuf> = if include_content {
            self.find_session_files()
                .into_iter()
                .map(|p| (Self::session_id_from_path(&p), p))
                .collect()
        } else {
            std::collections::HashMap::new()
        };

        // rayon 并行扫描：内容匹配需要读取所有会话文件，并行化以缩短搜索延迟
        use rayon::prelude::*;
        Ok(all
            .into_par_iter()
            .filter(|s| {
                // 先匹配标题/项目路径（便宜），再做会话内容全文匹配
                search::contains_ci(&s.title, query)
                    || s.project_path
                        .as_deref()
                        .is_some_and(|p| search::contains_ci(p, query))
                    || (include_content
                        && path_map
                            .get(&s.id)
                            .is_some_and(|p| self.file_content_matches(p, query)))
            })
            .collect())
    }
}

impl CodexProvider {
    /// 判断 JSONL 会话内容是否包含关键字
    ///
    /// 先做整文件原始字节预筛（绝大多数文件直接跳过），
    /// 命中后走完整解析——parse_session_file 已剔除 developer/环境上下文注入，
    /// 块级匹配再排除工具结果（见 search 模块），保证信噪比
    fn file_content_matches(&self, path: &PathBuf, query: &str) -> bool {
        let Ok(content) = fs::read_to_string(path) else {
            return false;
        };
        if !search::contains_ci(&content, query) {
            return false;
        }
        self.parse_session_file(path)
            .map(|(messages, _, _)| search::messages_match_query(&messages, query))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_points_to_home_codex() {
        let p = CodexProvider::default_path().unwrap();
        assert!(p.ends_with(".codex"));
    }

    #[test]
    fn new_with_override_uses_passed_path() {
        // 用一个临时存在的目录（系统临时目录一定存在）
        let tmp = std::env::temp_dir();
        let p = CodexProvider::new(Some(tmp.clone())).unwrap();
        assert_eq!(p.base_dir, tmp);
    }

    #[test]
    fn new_with_nonexistent_path_fails() {
        let bogus = std::path::PathBuf::from("/nonexistent/path/xyz123");
        assert!(CodexProvider::new(Some(bogus)).is_err());
    }

    /// 把 JSONL 内容写入临时文件并解析
    fn parse_fixture(name: &str, jsonl: &str) -> (Vec<Message>, Option<String>) {
        let dir = std::env::temp_dir();
        let path = dir.join(name);
        fs::write(&path, jsonl).unwrap();
        let provider = CodexProvider::new(Some(dir)).unwrap();
        let (messages, title, _) = provider.parse_session_file(&path).unwrap();
        fs::remove_file(&path).ok();
        (messages, title)
    }

    /// Codex ≥0.147：用户输入来自 event_msg.item_completed / item.type = UserMessage，
    /// 工具调用来自 custom_tool_call，工具输出为分段数组
    #[test]
    fn parses_user_message_from_item_completed() {
        let jsonl = r##"
{"timestamp":"2026-08-18T02:44:22.916Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions\n<INSTRUCTIONS>ignored</INSTRUCTIONS>"}]}}
{"timestamp":"2026-08-18T02:44:22.948Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"你好，帮我看下这个 bug"}]}}}
{"timestamp":"2026-08-18T02:44:27.671Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"好的"}]}}
{"timestamp":"2026-08-18T02:44:29.871Z","type":"response_item","payload":{"type":"custom_tool_call","call_id":"call_1","name":"exec","input":"const r = await tools.exec_command({cmd:\"ls\"});"}}
{"timestamp":"2026-08-18T02:44:30.560Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_1","output":[{"type":"input_text","text":"Script completed\n"},{"type":"input_text","text":"a.txt\n"}]}}
{"timestamp":"2026-08-18T02:44:31.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":[{"type":"text","text":"不应重复计入"}]}}}
"##;
        let (messages, title) = parse_fixture("codex-test-new.jsonl", jsonl);

        // 用户消息只出现一次，且取自 item_completed（不含 AGENTS.md 注入）
        let users: Vec<_> = messages.iter().filter(|m| m.role == Role::User).collect();
        assert_eq!(users.len(), 1);
        assert!(matches!(
            &users[0].content[0],
            ContentBlock::Text { text } if text == "你好，帮我看下这个 bug"
        ));
        assert_eq!(title.as_deref(), Some("你好，帮我看下这个 bug"));

        // custom_tool_call 的原始字符串 input 原样保留
        let tool_use = messages
            .iter()
            .flat_map(|m| &m.content)
            .find_map(|b| match b {
                ContentBlock::ToolUse { tool_name, input, .. } => Some((tool_name, input)),
                _ => None,
            })
            .expect("custom_tool_call 应解析为 ToolUse");
        assert_eq!(tool_use.0, "exec");
        assert!(tool_use.1.as_str().unwrap().contains("exec_command"));

        // 分段数组输出被拼接还原
        let result = messages
            .iter()
            .flat_map(|m| &m.content)
            .find_map(|b| match b {
                ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("custom_tool_call_output 应解析为 ToolResult");
        assert_eq!(result, "Script completed\na.txt\n");
    }

    /// Codex ≤0.146：用户输入来自 event_msg.user_message，工具输出为字符串
    #[test]
    fn parses_user_message_from_legacy_event() {
        let jsonl = r##"
{"timestamp":"2026-08-01T00:42:49.000Z","type":"event_msg","payload":{"type":"user_message","message":"旧格式的提问"}}
{"timestamp":"2026-08-01T00:42:50.000Z","type":"response_item","payload":{"type":"function_call","call_id":"call_2","name":"shell","arguments":"{\"cmd\":\"ls\"}"}}
{"timestamp":"2026-08-01T00:42:51.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_2","output":"a.txt\n"}}
"##;
        let (messages, title) = parse_fixture("codex-test-legacy.jsonl", jsonl);

        let users: Vec<_> = messages.iter().filter(|m| m.role == Role::User).collect();
        assert_eq!(users.len(), 1);
        assert!(matches!(
            &users[0].content[0],
            ContentBlock::Text { text } if text == "旧格式的提问"
        ));
        assert_eq!(title.as_deref(), Some("旧格式的提问"));

        let result = messages
            .iter()
            .flat_map(|m| &m.content)
            .find_map(|b| match b {
                ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(result, "a.txt\n");
    }
}
