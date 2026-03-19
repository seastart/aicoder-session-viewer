use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;

/// Claude Code 数据源
///
/// 存储结构：
/// - Session 索引: ~/.claude/sessions/*.json
/// - 对话内容: ~/.claude/projects/{path}/{uuid}.jsonl
pub struct ClaudeCodeProvider {
    /// ~/.claude 根目录
    base_dir: PathBuf,
}

impl ClaudeCodeProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir().ok_or_else(|| AppError::Provider("无法获取 home 目录".into()))?;
        let base_dir = home.join(".claude");
        if !base_dir.exists() {
            return Err(AppError::Provider("~/.claude 目录不存在".into()));
        }
        Ok(Self { base_dir })
    }

    /// 从 sessions 目录读取 session 索引文件
    fn read_session_indices(&self) -> AppResult<Vec<(String, serde_json::Value)>> {
        let sessions_dir = self.base_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut indices = Vec::new();
        for entry in fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        let id = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        indices.push((id, val));
                    }
                }
            }
        }
        Ok(indices)
    }

    /// 根据 session id 找到对应的 JSONL 文件路径
    fn find_jsonl_path(&self, session_id: &str) -> Option<PathBuf> {
        let projects_dir = self.base_dir.join("projects");
        if !projects_dir.exists() {
            return None;
        }

        // 遍历 projects 子目录寻找匹配的 JSONL 文件
        for entry in WalkDir::new(&projects_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Some(stem) = path.file_stem() {
                    if stem.to_string_lossy() == session_id {
                        return Some(path.to_path_buf());
                    }
                }
            }
        }
        None
    }

    /// 解析 JSONL 文件为消息列表
    fn parse_jsonl(&self, path: &PathBuf) -> AppResult<Vec<Message>> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(msg) = self.parse_message_entry(&entry) {
                    messages.push(msg);
                }
            }
        }
        Ok(messages)
    }

    /// 将单条 JSONL entry 解析为 Message
    fn parse_message_entry(&self, entry: &serde_json::Value) -> Option<Message> {
        // Claude Code JSONL 格式：
        // {"type":"human","message":{"role":"user","content":"..."}}
        // {"type":"assistant","message":{"role":"assistant","content":[...],"model":"..."}}
        let msg_type = entry.get("type")?.as_str()?;
        let message = entry.get("message")?;

        let role = match msg_type {
            "human" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => return None,
        };

        let content = self.parse_content(message.get("content")?);
        let model = message
            .get("model")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());

        // 解析 timestamp
        let timestamp = entry
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // 解析 token 用量
        let usage = message.get("usage").and_then(|u| {
            Some(TokenUsage {
                input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()),
                output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
                cache_read_tokens: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64()),
            })
        });

        let id = entry
            .get("uuid")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        Some(Message {
            id,
            role,
            content,
            timestamp,
            model,
            usage,
        })
    }

    /// 解析 content 字段（可能是 string 或 array）
    fn parse_content(&self, content: &serde_json::Value) -> Vec<ContentBlock> {
        match content {
            // 纯文本
            serde_json::Value::String(text) => {
                vec![ContentBlock::Text {
                    text: text.clone(),
                }]
            }
            // 结构化内容数组
            serde_json::Value::Array(blocks) => blocks
                .iter()
                .filter_map(|block| self.parse_content_block(block))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// 解析单个 content block
    fn parse_content_block(&self, block: &serde_json::Value) -> Option<ContentBlock> {
        let block_type = block.get("type")?.as_str()?;

        match block_type {
            "text" => {
                let text = block.get("text")?.as_str()?.to_string();
                Some(ContentBlock::Text { text })
            }
            "tool_use" => {
                let tool_name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let tool_id = block
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                Some(ContentBlock::ToolUse {
                    tool_name,
                    tool_id,
                    input,
                })
            }
            "tool_result" => {
                let tool_id = block
                    .get("tool_use_id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let is_error = block
                    .get("is_error")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);

                // content 可能是 string 或 array
                let content_text = match block.get("content") {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|item| {
                            if item.get("type")?.as_str()? == "text" {
                                item.get("text")?.as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };

                Some(ContentBlock::ToolResult {
                    tool_id,
                    content: content_text,
                    is_error,
                })
            }
            "thinking" => {
                let text = block.get("thinking")?.as_str()?.to_string();
                Some(ContentBlock::Thinking { text })
            }
            "image" => {
                let source = block
                    .get("source")
                    .and_then(|s| s.get("data"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let media_type = block
                    .get("source")
                    .and_then(|s| s.get("media_type"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());
                Some(ContentBlock::Image { source, media_type })
            }
            _ => None,
        }
    }

    /// 从 session 索引提取 title
    fn extract_title(index: &serde_json::Value) -> String {
        // 尝试从 session 索引中提取标题
        index
            .get("name")
            .or_else(|| index.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or("Untitled Session")
            .to_string()
    }

    /// 从 session 索引提取项目路径
    fn extract_project_path(index: &serde_json::Value) -> Option<String> {
        index
            .get("projectPath")
            .or_else(|| index.get("project_path"))
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
    }

    /// 构建 session id → JSONL 路径的映射（缓存避免重复遍历）
    fn build_jsonl_index(&self) -> HashMap<String, PathBuf> {
        let mut index = HashMap::new();
        let projects_dir = self.base_dir.join("projects");
        if !projects_dir.exists() {
            return index;
        }

        for entry in WalkDir::new(&projects_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Some(stem) = path.file_stem() {
                    index.insert(stem.to_string_lossy().to_string(), path.to_path_buf());
                }
            }
        }
        index
    }
}

impl SessionProvider for ClaudeCodeProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::ClaudeCode
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let indices = self.read_session_indices()?;
        let jsonl_map = self.build_jsonl_index();

        let mut summaries = Vec::new();
        for (id, index_val) in &indices {
            let title = Self::extract_title(index_val);
            let project_path = Self::extract_project_path(index_val);

            // 尝试获取消息数
            let message_count = jsonl_map
                .get(id)
                .and_then(|path| fs::read_to_string(path).ok())
                .map(|content| content.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0);

            // 尝试获取时间戳
            let started_at = index_val
                .get("createdAt")
                .or_else(|| index_val.get("created_at"))
                .and_then(|t| {
                    t.as_str()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .or_else(|| {
                            t.as_i64()
                                .and_then(|ms| DateTime::from_timestamp_millis(ms))
                        })
                });

            summaries.push(SessionSummary {
                id: id.clone(),
                tool: ToolKind::ClaudeCode,
                title,
                project_path,
                started_at,
                message_count,
            });
        }

        // 按时间倒序
        summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        // 先获取 session 索引信息
        let indices = self.read_session_indices()?;
        let index_val = indices
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, v)| v.clone());

        let title = index_val
            .as_ref()
            .map(|v| Self::extract_title(v))
            .unwrap_or_else(|| "Untitled Session".to_string());
        let project_path = index_val.as_ref().and_then(|v| Self::extract_project_path(v));

        // 找到并解析 JSONL 文件
        let jsonl_path = self
            .find_jsonl_path(session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
        let messages = self.parse_jsonl(&jsonl_path)?;

        let started_at = messages.first().and_then(|m| m.timestamp);

        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::ClaudeCode,
            title,
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
            .filter(|s| {
                s.title.to_lowercase().contains(&query_lower)
                    || s.project_path
                        .as_deref()
                        .is_some_and(|p| p.to_lowercase().contains(&query_lower))
            })
            .collect())
    }
}
