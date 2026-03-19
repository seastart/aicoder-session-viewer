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
pub struct GeminiProvider {
    base_dir: PathBuf,
}

impl GeminiProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir().ok_or_else(|| AppError::Provider("无法获取 home 目录".into()))?;
        let base_dir = home.join(".gemini");
        if !base_dir.exists() {
            return Err(AppError::Provider("~/.gemini 目录不存在".into()));
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

    /// 从文件路径提取项目目录名
    fn project_from_path(path: &PathBuf) -> Option<String> {
        // 路径格式: ~/.gemini/tmp/{project}/chats/session-*.json
        path.parent()? // chats/
            .parent()? // {project}/
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }

    /// 解析 Gemini session JSON 文件
    fn parse_session_file(&self, path: &PathBuf) -> AppResult<(Vec<Message>, Option<String>)> {
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let mut messages = Vec::new();
        let mut title = None;

        // 尝试提取标题
        if let Some(t) = data.get("title").and_then(|t| t.as_str()) {
            title = Some(t.to_string());
        }

        // 解析消息列表
        let msg_array = data
            .get("messages")
            .or_else(|| data.get("history"))
            .and_then(|m| m.as_array());

        if let Some(msgs) = msg_array {
            for (i, msg) in msgs.iter().enumerate() {
                if let Some(parsed) = self.parse_message(msg, i) {
                    messages.push(parsed);
                }
            }
        }

        Ok((messages, title))
    }

    /// 解析单条 Gemini 消息
    fn parse_message(&self, msg: &serde_json::Value, index: usize) -> Option<Message> {
        let role_str = msg.get("role")?.as_str()?;
        let role = match role_str {
            "user" => Role::User,
            "model" | "assistant" => Role::Assistant,
            _ => return None,
        };

        let mut content_blocks = Vec::new();

        // Gemini 的 content 可能是 parts 数组
        let parts = msg
            .get("parts")
            .or_else(|| msg.get("content"))
            .and_then(|p| p.as_array());

        if let Some(parts) = parts {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    content_blocks.push(ContentBlock::Text {
                        text: text.to_string(),
                    });
                } else if let Some(fc) = part.get("functionCall") {
                    let name = fc
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let args = fc.get("args").cloned().unwrap_or(serde_json::Value::Null);
                    content_blocks.push(ContentBlock::ToolUse {
                        tool_name: name,
                        tool_id: None,
                        input: args,
                    });
                } else if let Some(fr) = part.get("functionResponse") {
                    let response = fr
                        .get("response")
                        .map(|r| serde_json::to_string_pretty(r).unwrap_or_default())
                        .unwrap_or_default();
                    content_blocks.push(ContentBlock::ToolResult {
                        tool_id: None,
                        content: response,
                        is_error: false,
                    });
                } else if let Some(thought) = part.get("thought").and_then(|t| t.as_str()) {
                    content_blocks.push(ContentBlock::Thinking {
                        text: thought.to_string(),
                    });
                }
            }
        } else if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
            // content 直接是字符串
            content_blocks.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }

        if content_blocks.is_empty() {
            return None;
        }

        Some(Message {
            id: format!("gemini-msg-{}", index),
            role,
            content: content_blocks,
            timestamp: None,
            model: msg
                .get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string()),
            usage: None,
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
            let id = Self::session_id_from_path(path);
            let project = Self::project_from_path(path);

            // 快速解析获取摘要信息（不解析全部消息）
            let (msg_count, title, started_at) = match fs::read_to_string(path) {
                Ok(content) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&content).unwrap_or_default();
                    let count = data
                        .get("messages")
                        .or_else(|| data.get("history"))
                        .and_then(|m| m.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let title = data
                        .get("title")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());
                    let started = data
                        .get("createTime")
                        .and_then(|t| t.as_str())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                    (count, title, started)
                }
                Err(_) => (0, None, None),
            };

            // 如果没有时间戳，用文件修改时间
            let started_at = started_at.or_else(|| {
                fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| DateTime::<Utc>::from(t))
            });

            summaries.push(SessionSummary {
                id,
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
        let path = files
            .iter()
            .find(|p| Self::session_id_from_path(p) == session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;

        let (messages, title) = self.parse_session_file(path)?;
        let project = Self::project_from_path(path);

        let started_at = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| DateTime::<Utc>::from(t));

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
