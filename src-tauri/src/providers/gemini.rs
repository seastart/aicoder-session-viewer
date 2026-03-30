use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;

/// Gemini CLI 数据源
///
/// 存储结构：~/.gemini/tmp/{project}/chats/session-*.json
/// 每个 JSON 文件包含一个完整 session
///
/// 消息格式（Gemini CLI 自有格式，非 Google AI API 格式）：
/// - `type`: "user" | "gemini" | "info" | "error"
/// - `content`: user 消息为 [{text: "..."}] 数组，gemini 消息为字符串
/// - `toolCalls`: 工具调用数组，每项含 id/name/args/result
/// - `thoughts`: 思考过程数组，每项含 subject/description
/// - `tokens`: {input, output, cached, thoughts, tool, total}
/// - `timestamp`: ISO 8601 时间戳
pub struct GeminiProvider {
    base_dir: PathBuf,
}

impl GeminiProvider {
    pub fn new() -> AppResult<Self> {
        let home =
            dirs::home_dir().ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        let base_dir = home.join(".gemini");
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}", base_dir.display()
            )));
        }
        Ok(Self { base_dir })
    }

    /// 扫描所有 session JSON 文件
    fn find_session_files(&self) -> Vec<PathBuf> {
        let tmp_dir = self.base_dir.join("tmp");
        if !tmp_dir.exists() {
            return Vec::new();
        }

        WalkDir::new(&tmp_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("session-"))
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

    /// 从文件路径提取真实项目路径
    ///
    /// 路径格式: ~/.gemini/tmp/{project}/chats/session-*.json
    /// 真实路径存储在: ~/.gemini/tmp/{project}/.project_root
    fn project_from_path(path: &PathBuf) -> Option<String> {
        let project_dir = path.parent()?.parent()?; // ~/.gemini/tmp/{project}/
        let project_root_file = project_dir.join(".project_root");

        // 优先读取 .project_root 获取真实完整路径
        if let Ok(content) = fs::read_to_string(&project_root_file) {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        // 兜底：返回目录名
        project_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }

    /// 根据 session_id 查找对应的文件
    ///
    /// 优先匹配 JSON 中的 sessionId 字段（真实 UUID），
    /// 其次匹配文件名（向后兼容）
    fn find_file_by_session_id(&self, files: &[PathBuf], session_id: &str) -> Option<PathBuf> {
        // 先尝试文件名匹配（快速路径，向后兼容旧 ID 格式）
        if let Some(path) = files.iter().find(|p| Self::session_id_from_path(p) == session_id) {
            return Some(path.clone());
        }

        // 再逐个读取 JSON 匹配真实 sessionId
        for path in files {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if data.get("sessionId").and_then(|v| v.as_str()) == Some(session_id) {
                        return Some(path.clone());
                    }
                }
            }
        }

        None
    }

    /// 从第一条用户消息中提取标题（截取前 60 个字符）
    fn extract_title_from_messages(msgs: &[serde_json::Value]) -> Option<String> {
        for msg in msgs {
            if msg.get("type").and_then(|t| t.as_str()) != Some("user") {
                continue;
            }
            // user 消息的 content 是 [{text: "..."}] 数组
            if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
                if let Some(text) = arr.first().and_then(|item| item.get("text")).and_then(|t| t.as_str()) {
                    // 去掉开头的 @ 引用（如 "@report.docx"），取有意义的部分
                    let clean = text.trim();
                    if clean.is_empty() {
                        continue;
                    }
                    // 截取第一行，最多 60 字符
                    let first_line = clean.lines().next().unwrap_or(clean);
                    let truncated: String = first_line.chars().take(60).collect();
                    if truncated.len() < first_line.len() {
                        return Some(format!("{}...", truncated));
                    }
                    return Some(truncated);
                }
            }
        }
        None
    }

    /// 解析 ISO 8601 时间戳
    fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// 解析 Gemini session JSON 文件，返回 (消息列表, 标题)
    fn parse_session_file(&self, path: &PathBuf) -> AppResult<(Vec<Message>, Option<String>)> {
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let mut messages = Vec::new();

        let msg_array = data.get("messages").and_then(|m| m.as_array());

        // 从第一条用户消息提取标题
        let title = msg_array.and_then(|m| Self::extract_title_from_messages(m));

        if let Some(msgs) = msg_array {
            for (i, msg) in msgs.iter().enumerate() {
                if let Some(parsed) = self.parse_message(msg, i) {
                    messages.push(parsed);
                }
            }
        }

        Ok((messages, title))
    }

    /// 解析单条 Gemini CLI 消息
    fn parse_message(&self, msg: &serde_json::Value, index: usize) -> Option<Message> {
        let msg_type = msg.get("type")?.as_str()?;

        let role = match msg_type {
            "user" => Role::User,
            "gemini" => Role::Assistant,
            // info/error 作为系统消息展示
            "info" | "error" => Role::System,
            _ => return None,
        };

        let mut content_blocks = Vec::new();

        match msg_type {
            "user" => {
                // user 消息: content 是 [{text: "..."}] 数组
                if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            content_blocks.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                }
            }
            "gemini" => {
                // 1. 思考过程 (thoughts 数组)
                if let Some(thoughts) = msg.get("thoughts").and_then(|t| t.as_array()) {
                    for thought in thoughts {
                        let subject = thought
                            .get("subject")
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        let desc = thought
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");
                        // 将 subject + description 合并为思考内容
                        let text = if subject.is_empty() {
                            desc.to_string()
                        } else {
                            format!("**{}**\n{}", subject, desc)
                        };
                        if !text.is_empty() {
                            content_blocks.push(ContentBlock::Thinking { text });
                        }
                    }
                }

                // 2. 工具调用 (toolCalls 数组)
                if let Some(tool_calls) = msg.get("toolCalls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let name = tc
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let tool_id = tc
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string());
                        let args = tc.get("args").cloned().unwrap_or(serde_json::Value::Null);

                        content_blocks.push(ContentBlock::ToolUse {
                            tool_name: name,
                            tool_id: tool_id.clone(),
                            input: args,
                            agent_id: None,
                        });

                        // 工具结果内嵌在 result 数组中
                        if let Some(results) = tc.get("result").and_then(|r| r.as_array()) {
                            for res in results {
                                if let Some(fr) = res.get("functionResponse") {
                                    let output = fr
                                        .get("response")
                                        .and_then(|r| {
                                            // 优先取 output 字段，否则序列化整个 response
                                            r.get("output")
                                                .and_then(|o| o.as_str())
                                                .map(|s| s.to_string())
                                                .or_else(|| {
                                                    r.get("error")
                                                        .and_then(|e| e.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                                .or_else(|| {
                                                    serde_json::to_string_pretty(r).ok()
                                                })
                                        })
                                        .unwrap_or_default();
                                    let is_error = fr
                                        .get("response")
                                        .and_then(|r| r.get("error"))
                                        .is_some();
                                    content_blocks.push(ContentBlock::ToolResult {
                                        tool_id: tool_id.clone(),
                                        content: output,
                                        is_error,
                                    });
                                }
                            }
                        }
                    }
                }

                // 3. 正文内容 (content 字符串)
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
            "info" | "error" => {
                // info/error 消息: content 是纯字符串
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        let prefix = if msg_type == "error" { "⚠ " } else { "" };
                        content_blocks.push(ContentBlock::Text {
                            text: format!("{}{}", prefix, text),
                        });
                    }
                }
            }
            _ => {}
        }

        if content_blocks.is_empty() {
            return None;
        }

        // 解析时间戳
        let timestamp = msg
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(Self::parse_timestamp);

        // 解析 token 用量
        let usage = msg.get("tokens").map(|tokens| TokenUsage {
            input_tokens: tokens.get("input").and_then(|v| v.as_u64()),
            output_tokens: tokens.get("output").and_then(|v| v.as_u64()),
            cache_read_tokens: tokens.get("cached").and_then(|v| v.as_u64()),
        });

        // 使用消息自带的 id
        let id = msg
            .get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("gemini-msg-{}", index));

        Some(Message {
            id,
            role,
            content: content_blocks,
            timestamp,
            model: msg
                .get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string()),
            usage,
        })
    }
}

impl SessionProvider for GeminiProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::Gemini
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let files = self.find_session_files();
        let mut summaries = Vec::new();

        for path in &files {
            let fallback_id = Self::session_id_from_path(path);
            let project = Self::project_from_path(path);

            // 解析 JSON，提取真实 sessionId 和摘要信息
            let (real_id, msg_count, title, started_at) = match fs::read_to_string(path) {
                Ok(content) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&content).unwrap_or_default();

                    // 使用 JSON 中的 sessionId 作为真实 ID
                    let real_id = data
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let msgs = data.get("messages").and_then(|m| m.as_array());
                    let count = msgs.map(|a| a.len()).unwrap_or(0);
                    let title = msgs.and_then(|m| Self::extract_title_from_messages(m));
                    let started = data
                        .get("startTime")
                        .and_then(|t| t.as_str())
                        .and_then(Self::parse_timestamp);

                    (real_id, count, title, started)
                }
                Err(_) => (None, 0, None, None),
            };

            // 如果没有 startTime，用文件修改时间
            let started_at = started_at.or_else(|| {
                fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| DateTime::<Utc>::from(t))
            });

            summaries.push(SessionSummary {
                id: real_id.unwrap_or(fallback_id),
                tool: ToolKind::Gemini,
                title: title.unwrap_or_else(|| {
                    project
                        .clone()
                        .unwrap_or_else(|| "Gemini Session".to_string())
                }),
                project_path: project,
                started_at,
                message_count: msg_count,
            });
        }

        summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        let files = self.find_session_files();

        // 查找匹配的文件：先匹配真实 sessionId（从 JSON），再匹配文件名
        let path = self
            .find_file_by_session_id(&files, session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;

        let (messages, title) = self.parse_session_file(&path)?;
        let project = Self::project_from_path(&path);

        let started_at = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|data| {
                data.get("startTime")
                    .and_then(|t| t.as_str().map(|s| s.to_string()))
            })
            .and_then(|s| Self::parse_timestamp(&s))
            .or_else(|| {
                fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| DateTime::<Utc>::from(t))
            });

        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::Gemini,
            title: title.unwrap_or_else(|| {
                project
                    .clone()
                    .unwrap_or_else(|| "Gemini Session".to_string())
            }),
            project_path: project,
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
            .filter(|s| {
                s.title.to_lowercase().contains(&query_lower)
                    || s.project_path
                        .as_deref()
                        .is_some_and(|p| p.to_lowercase().contains(&query_lower))
            })
            .collect())
    }
}
