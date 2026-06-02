use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::{search, SessionProvider};
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
    /// 按文件 mtime 缓存的会话摘要：文件未变化时跳过读取与解析，
    /// 列表/搜索从「每次全量扫描」降为「只处理有变化的文件」
    summary_cache: std::sync::RwLock<HashMap<PathBuf, (std::time::SystemTime, SessionSummary)>>,
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
    /// 返回默认的 ~/.claude 路径
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".claude"))
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
        Ok(Self {
            base_dir,
            summary_cache: std::sync::RwLock::new(HashMap::new()),
        })
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

    /// 单次遍历 JSONL 内容，一趟提取摘要所需的全部字段：
    /// (标题, 消息数, cwd, 起始时间)
    ///
    /// 旧实现 title/count/cwd/timestamp 各自完整读一遍文件（4 次全量 IO），
    /// 是列表与实时搜索卡顿的主要来源，这里合并为单次读取单次遍历；
    /// 且仅在还缺字段时才对行做 JSON 解析
    fn scan_summary_from_content(
        content: &str,
    ) -> (String, usize, Option<String>, Option<DateTime<Utc>>) {
        let mut title: Option<String> = None;
        let mut cwd: Option<String> = None;
        let mut started_at: Option<DateTime<Utc>> = None;
        let mut count = 0usize;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 消息计数沿用廉价的原始字符串判断（与旧 count_messages 行为一致）
            let is_user = line.contains("\"type\":\"user\"") || line.contains("\"type\":\"human\"");
            if is_user || line.contains("\"type\":\"assistant\"") {
                count += 1;
            }

            // 所有字段都已拿到时，只剩计数，无需再做 JSON 解析
            let need_parse =
                cwd.is_none() || started_at.is_none() || (title.is_none() && is_user);
            if !need_parse {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };

            // cwd / timestamp：很多行都带，取第一个出现的即可
            if cwd.is_none() {
                if let Some(c) = entry.get("cwd").and_then(|c| c.as_str()) {
                    cwd = Some(c.to_string());
                }
            }
            if started_at.is_none() {
                if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                        started_at = Some(dt.with_timezone(&Utc));
                    }
                }
            }
            if title.is_none() && is_user {
                title = Self::title_from_user_entry(&entry);
            }
        }

        (
            title.unwrap_or_else(|| "Untitled Session".to_string()),
            count,
            cwd,
            started_at,
        )
    }

    /// 从单条用户消息 entry 中提取标题候选（截取前 80 字符）
    ///
    /// 返回 None 表示该条不适合做标题，继续找下一条真正的用户消息：
    /// - 无文本的消息（如仅含 tool_result）
    /// - `<` 开头：Claude Code 注入的 XML 系统内容（如 `<local-command-caveat>`）
    /// - `[` 开头：系统通知（如 `[Request interrupted by user for tool use]`）
    fn title_from_user_entry(entry: &serde_json::Value) -> Option<String> {
        let content_val = entry.get("message").and_then(|m| m.get("content"))?;
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
        if trimmed.is_empty() || trimmed.starts_with('<') || trimmed.starts_with('[') {
            return None;
        }
        // 多行合并为一行（空格连接），截取前 80 字符
        let oneline: String = trimmed
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let title: String = oneline.chars().take(80).collect();
        Some(if title.len() < oneline.len() {
            format!("{}...", title)
        } else {
            title
        })
    }

    /// 获取单个会话的摘要（带 mtime 缓存：文件未变化时直接复用上次结果）
    fn summary_for(&self, info: &JsonlFileInfo) -> SessionSummary {
        let mtime = fs::metadata(&info.path).ok().and_then(|m| m.modified().ok());

        // 缓存命中且文件未变化 → 直接复用
        if let Some(mt) = mtime {
            if let Some((cached_mt, cached)) =
                self.summary_cache.read().unwrap().get(&info.path)
            {
                if *cached_mt == mt {
                    return cached.clone();
                }
            }
        }

        let content = fs::read_to_string(&info.path).unwrap_or_default();
        let (title, message_count, cwd, started_at) = Self::scan_summary_from_content(&content);
        // 优先从 JSONL 内容取 cwd，否则从目录名反推
        let project_path = cwd.unwrap_or_else(|| Self::dir_name_to_path(&info.project_dir_name));

        let summary = SessionSummary {
            id: info.session_id.clone(),
            tool: ToolKind::ClaudeCode,
            title,
            project_path: Some(project_path),
            started_at,
            // 文件修改时间作为最后活跃时间
            updated_at: mtime.map(DateTime::<Utc>::from),
            message_count,
            total_tokens: None,
        };

        if let Some(mt) = mtime {
            self.summary_cache
                .write()
                .unwrap()
                .insert(info.path.clone(), (mt, summary.clone()));
        }
        summary
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
            // Claude API 的 input_tokens 仅含未缓存部分，需要加上缓存读取和新建缓存
            let raw_input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cache_read = u.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cache_creation = u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let total_input = raw_input + cache_read + cache_creation;

            Some(TokenUsage {
                input_tokens: Some(total_input),
                output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
                cache_read_tokens: if cache_read > 0 { Some(cache_read) } else { None },
                cache_creation_tokens: if cache_creation > 0 { Some(cache_creation) } else { None },
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

        // rayon 并行：冷启动（缓存未建立）时需要读取所有文件，并行化缩短首次延迟；
        // 缓存命中后每个文件只剩一次 stat，开销极小
        use rayon::prelude::*;
        let mut summaries: Vec<SessionSummary> =
            files.par_iter().map(|info| self.summary_for(info)).collect();

        // 按最后活跃时间倒序（没有 updated_at 时回退到 started_at）
        summaries.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.started_at);
            let b_time = b.updated_at.or(b.started_at);
            b_time.cmp(&a_time)
        });
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
        // 标题/项目路径复用带缓存的摘要提取
        let cached = self.summary_for(info);
        let started_at = messages.first().and_then(|m| m.timestamp);
        let updated_at = messages.last().and_then(|m| m.timestamp);

        let total_tokens = sum_message_tokens(&messages);
        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::ClaudeCode,
            title: cached.title,
            project_path: cached.project_path,
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
        // session_id → 文件路径映射，供内容全文匹配使用（仅深度搜索时构建）
        let path_map: HashMap<String, PathBuf> = if include_content {
            self.scan_main_sessions()
                .into_iter()
                .map(|f| (f.session_id, f.path))
                .collect()
        } else {
            HashMap::new()
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

impl ClaudeCodeProvider {
    /// 判断 JSONL 会话内容是否包含关键字
    ///
    /// 两级过滤避免全量 JSON 解析的开销：
    /// 1. 整文件原始字节预筛——绝大多数不含关键字的文件直接跳过
    /// 2. 仅对原始命中的行做 JSON 解析 + 块级精确匹配
    ///    （排除 system-reminder 注入和工具结果，见 search 模块），命中即早退
    fn file_content_matches(&self, path: &PathBuf, query: &str) -> bool {
        let Ok(content) = fs::read_to_string(path) else {
            return false;
        };
        if !search::contains_ci(&content, query) {
            return false;
        }
        content.lines().any(|line| {
            let line = line.trim();
            if line.is_empty() || !search::contains_ci(line, query) {
                return false;
            }
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|entry| self.parse_message_entry(&entry))
                .is_some_and(|msg| search::message_matches_query(&msg, query))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_points_to_home_claude() {
        let p = ClaudeCodeProvider::default_path().unwrap();
        assert!(p.ends_with(".claude"));
    }

    #[test]
    fn new_with_override_uses_passed_path() {
        // 用一个临时存在的目录（系统临时目录一定存在）
        let tmp = std::env::temp_dir();
        let p = ClaudeCodeProvider::new(Some(tmp.clone())).unwrap();
        assert_eq!(p.base_dir, tmp);
    }

    #[test]
    fn new_with_nonexistent_path_fails() {
        let bogus = std::path::PathBuf::from("/nonexistent/path/xyz123");
        assert!(ClaudeCodeProvider::new(Some(bogus)).is_err());
    }
}
