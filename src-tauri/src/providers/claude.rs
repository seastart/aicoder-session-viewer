use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::SessionProvider;
use chrono::{DateTime, Utc};

/// Claude Code 数据源
///
/// 存储结构：
/// - 主 session: ~/.claude/projects/{project-path}/{uuid}.jsonl
/// - 子代理:    ~/.claude/projects/{project-path}/{uuid}/subagents/agent-xxx.jsonl
/// - 活跃索引:  ~/.claude/sessions/*.json（仅当前运行中的进程，不作为主数据源）
///
/// 发现策略：直接扫描 projects 目录下的 JSONL 文件，排除 subagents 子目录
pub struct ClaudeCodeProvider {
    /// ~/.claude 根目录
    base_dir: PathBuf,
}

/// 扫描到的 JSONL 文件信息
struct JsonlFileInfo {
    /// session UUID（文件名去掉 .jsonl）
    session_id: String,
    /// JSONL 文件完整路径
    path: PathBuf,
    /// 所属项目目录名（如 -Users-shushenghong-Documents-workspace-srtc）
    project_dir_name: String,
}

impl ClaudeCodeProvider {
    pub fn new() -> AppResult<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        let base_dir = home.join(".claude");
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}",
                base_dir.display()
            )));
        }
        Ok(Self { base_dir })
    }

    /// 扫描 projects 目录，收集所有主 session 的 JSONL 文件（排除 subagents）
    fn scan_main_sessions(&self) -> Vec<JsonlFileInfo> {
        let projects_dir = self.base_dir.join("projects");
        if !projects_dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();

        // 遍历 projects 下的每个项目目录
        for project_entry in fs::read_dir(&projects_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
        {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let project_dir_name = project_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // 只扫描项目目录下的直接 .jsonl 文件（非 subagents）
            for file_entry in fs::read_dir(&project_path)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
            {
                let file_path = file_entry.path();
                if file_path.extension().is_some_and(|e| e == "jsonl") && file_path.is_file() {
                    let session_id = file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    results.push(JsonlFileInfo {
                        session_id,
                        path: file_path,
                        project_dir_name: project_dir_name.clone(),
                    });
                }
            }
        }
        results
    }

    /// 将项目目录名还原为实际路径
    ///
    /// macOS/Linux: "-Users-shushenghong-Documents-project" → "/Users/shushenghong/Documents/project"
    /// Windows:     "-C-Users-shushenghong-project"         → "C:/Users/shushenghong/project"
    fn dir_name_to_path(dir_name: &str) -> String {
        if !dir_name.starts_with('-') {
            return dir_name.replace('-', "/");
        }

        let raw = format!("/{}", &dir_name[1..]).replace('-', "/");

        // Windows 盘符检测："/C/Users/..." → "C:/Users/..."
        // 模式为 /X/ 其中 X 是单个字母
        if raw.len() >= 3 {
            let bytes = raw.as_bytes();
            if bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b'/' {
                let drive = bytes[1] as char;
                return format!("{}:{}", drive, &raw[2..]);
            }
        }

        raw
    }

    /// 从 JSONL 第一条消息中提取项目路径（cwd 字段）
    fn extract_cwd_from_jsonl(path: &PathBuf) -> Option<String> {
        let content = fs::read_to_string(path).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                // 很多行都带有 cwd 字段，取第一个就行
                if let Some(cwd) = entry.get("cwd").and_then(|c| c.as_str()) {
                    return Some(cwd.to_string());
                }
            }
        }
        None
    }

    /// 从 JSONL 第一条消息中提取时间戳
    fn extract_timestamp_from_jsonl(path: &PathBuf) -> Option<DateTime<Utc>> {
        let content = fs::read_to_string(path).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                        return Some(dt.with_timezone(&Utc));
                    }
                }
            }
        }
        None
    }

    /// 从 JSONL 文件中提取标题（取第一条有实际内容的用户消息，截取前 80 字符）
    ///
    /// 跳过以下非用户内容，继续找下一条真正的用户消息：
    /// - 无文本的消息（如仅含 tool_result）
    /// - `<` 开头：Claude Code 注入的 XML 系统内容（如 `<local-command-caveat>`）
    /// - `[` 开头：系统通知（如 `[Request interrupted by user for tool use]`）
    fn extract_title_from_jsonl(path: &PathBuf) -> String {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    let msg_type = entry.get("type").and_then(|t| t.as_str());
                    if matches!(msg_type, Some("user") | Some("human")) {
                        if let Some(content_val) =
                            entry.get("message").and_then(|m| m.get("content"))
                        {
                            let text = match content_val {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Array(arr) => arr
                                    .iter()
                                    .find_map(|block| {
                                        if block.get("type")?.as_str()? == "text" {
                                            block.get("text")?.as_str().map(|s| s.to_string())
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_default(),
                                _ => String::new(),
                            };
                            let trimmed = text.trim();
                            // 跳过：空文本、XML 系统注入、系统通知（如 [Request interrupted...]）
                            if trimmed.is_empty()
                                || trimmed.starts_with('<')
                                || trimmed.starts_with('[')
                            {
                                continue;
                            }
                            // 多行合并为一行（空格连接），截取前 80 字符
                            let oneline: String = trimmed
                                .lines()
                                .map(|l| l.trim())
                                .filter(|l| !l.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ");
                            let title: String = oneline.chars().take(80).collect();
                            return if title.len() < oneline.len() {
                                format!("{}...", title)
                            } else {
                                title
                            };
                        }
                    }
                }
            }
        }
        "Untitled Session".to_string()
    }

    /// 统计 JSONL 文件中的消息数（只计 user/assistant 类型）
    fn count_messages(path: &PathBuf) -> usize {
        fs::read_to_string(path)
            .map(|content| {
                content
                    .lines()
                    .filter(|l| {
                        let l = l.trim();
                        l.contains("\"type\":\"user\"")
                            || l.contains("\"type\":\"human\"")
                            || l.contains("\"type\":\"assistant\"")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// 解析 JSONL 文件为消息列表
    /// 同时从 progress 条目中提取 tool_use_id → agent_id 映射，注入到 ToolUse block 中
    fn parse_jsonl(&self, path: &PathBuf) -> AppResult<Vec<Message>> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();
        // tool_use_id → agent_id 映射（从 progress 条目中提取）
        let mut agent_map: HashMap<String, String> = HashMap::new();

        // 第一遍：收集 agent_id 映射
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if entry.get("type").and_then(|t| t.as_str()) == Some("progress") {
                    let parent_tool_id = entry.get("parentToolUseID").and_then(|v| v.as_str());
                    let agent_id = entry
                        .get("data")
                        .and_then(|d| d.get("agentId"))
                        .and_then(|v| v.as_str());
                    if let (Some(tid), Some(aid)) = (parent_tool_id, agent_id) {
                        if !aid.is_empty() {
                            agent_map.insert(tid.to_string(), aid.to_string());
                        }
                    }
                }
            }
        }

        // 第二遍：解析消息，并注入 agent_id
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(mut msg) = self.parse_message_entry(&entry) {
                    // 为 Agent 类型的 ToolUse 注入 agent_id
                    for block in &mut msg.content {
                        if let ContentBlock::ToolUse {
                            tool_name,
                            tool_id,
                            agent_id,
                            ..
                        } = block
                        {
                            if tool_name == "Agent" {
                                if let Some(tid) = tool_id {
                                    if let Some(aid) = agent_map.get(tid.as_str()) {
                                        *agent_id = Some(aid.clone());
                                    }
                                }
                            }
                        }
                    }
                    messages.push(msg);
                }
            }
        }
        Ok(messages)
    }

    /// 将单条 JSONL entry 解析为 Message
    fn parse_message_entry(&self, entry: &serde_json::Value) -> Option<Message> {
        let msg_type = entry.get("type")?.as_str()?;
        let message = entry.get("message")?;

        let role = match msg_type {
            "human" | "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            // 跳过 file-history-snapshot 等非消息类型
            _ => return None,
        };

        let content = self.parse_content(message.get("content")?);
        let model = message
            .get("model")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());

        let timestamp = entry
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let usage = message.get("usage").and_then(|u| {
            Some(TokenUsage {
                input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()),
                output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
                cache_read_tokens: u.get("cache_read_input_tokens").and_then(|v| v.as_u64()),
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
            serde_json::Value::String(text) => {
                vec![ContentBlock::Text { text: text.clone() }]
            }
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
                let input = block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(ContentBlock::ToolUse {
                    tool_name,
                    tool_id,
                    input,
                    agent_id: None, // 后续由 parse_jsonl 从 progress 条目注入
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

    /// 加载指定 subagent 的对话消息
    pub fn get_subagent_messages(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> AppResult<Vec<Message>> {
        let projects_dir = self.base_dir.join("projects");
        if !projects_dir.exists() {
            return Err(AppError::SessionNotFound(session_id.to_string()));
        }

        // 在所有项目目录下查找 {session_id}/subagents/agent-{agent_id}.jsonl
        for project_entry in fs::read_dir(&projects_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
        {
            let subagent_path = project_entry
                .path()
                .join(session_id)
                .join("subagents")
                .join(format!("agent-{}.jsonl", agent_id));

            if subagent_path.exists() {
                return self.parse_jsonl(&subagent_path);
            }
        }

        Err(AppError::SessionNotFound(format!(
            "subagent {} in session {}",
            agent_id, session_id
        )))
    }
}

impl SessionProvider for ClaudeCodeProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::ClaudeCode
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let files = self.scan_main_sessions();

        let mut summaries = Vec::new();
        for info in &files {
            let title = Self::extract_title_from_jsonl(&info.path);
            let message_count = Self::count_messages(&info.path);

            // 优先从 JSONL 内容取 cwd，否则从目录名反推
            let project_path = Self::extract_cwd_from_jsonl(&info.path)
                .unwrap_or_else(|| Self::dir_name_to_path(&info.project_dir_name));

            // 从 JSONL 内容取时间戳
            let started_at = Self::extract_timestamp_from_jsonl(&info.path);

            summaries.push(SessionSummary {
                id: info.session_id.clone(),
                tool: ToolKind::ClaudeCode,
                title,
                project_path: Some(project_path),
                started_at,
                message_count,
            });
        }

        // 按时间倒序
        summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        // 从扫描结果中找到对应文件
        let files = self.scan_main_sessions();
        let info = files
            .iter()
            .find(|f| f.session_id == session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;

        let messages = self.parse_jsonl(&info.path)?;
        let title = Self::extract_title_from_jsonl(&info.path);
        let project_path = Self::extract_cwd_from_jsonl(&info.path)
            .unwrap_or_else(|| Self::dir_name_to_path(&info.project_dir_name));
        let started_at = messages.first().and_then(|m| m.timestamp);

        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::ClaudeCode,
            title,
            project_path: Some(project_path),
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
