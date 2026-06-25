use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::providers::{search, SessionProvider};

/// Antigravity CLI 数据源
///
/// 存储结构：~/.gemini/antigravity-cli/brain/{conversation-id}/.system_generated/logs/transcript.jsonl
/// `transcript.jsonl` 是首选入口；如果某个会话缺少它，再退回 `transcript_full.jsonl`。
pub struct AntigravityProvider {
    base_dir: PathBuf,
    summary_cache: std::sync::RwLock<
        HashMap<PathBuf, (std::time::SystemTime, Option<String>, SessionSummary)>,
    >,
}

impl AntigravityProvider {
    /// 返回默认的 ~/.gemini/antigravity-cli 路径
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".gemini").join("antigravity-cli"))
    }

    /// 创建 provider；`path_override` 为 None 时走默认路径
    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = path_override.unwrap_or(Self::default_path()?);
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

    /// 扫描 Antigravity brain 下的 transcript 文件。
    ///
    /// 首版只解析可读 JSONL，不反解 conversations/*.db 的内部 BLOB；
    /// 这样可以让历史浏览依赖最稳定、最透明的数据层。
    fn find_transcript_files(&self) -> Vec<PathBuf> {
        let brain_dir = self.base_dir.join("brain");
        if !brain_dir.exists() {
            return Vec::new();
        }

        let mut files = Vec::new();
        let Ok(entries) = fs::read_dir(brain_dir) else {
            return files;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let logs_dir = path.join(".system_generated").join("logs");
            let transcript = logs_dir.join("transcript.jsonl");
            if transcript.exists() {
                files.push(transcript);
                continue;
            }
            let full = logs_dir.join("transcript_full.jsonl");
            if full.exists() {
                files.push(full);
            }
        }

        files
    }

    fn conversation_id_from_path(path: &Path) -> Option<String> {
        path.parent()? // logs
            .parent()? // .system_generated
            .parent()? // conversation id
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }

    /// 从 history/cache 中补充 conversation -> workspace 映射。
    ///
    /// transcript 本身未必记录工作目录；Antigravity 的 history.jsonl 和
    /// cache/last_conversations.json 都是“会话选择/恢复”层面的索引，适合作为
    /// project_path 的非强制补充。缺失时返回 None，不影响会话读取。
    fn workspace_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();

        let history_path = self.base_dir.join("history.jsonl");
        if let Ok(content) = fs::read_to_string(history_path) {
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                let Ok(v) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                let Some(conversation_id) = v.get("conversationId").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(workspace) = v.get("workspace").and_then(|v| v.as_str()) else {
                    continue;
                };
                if !workspace.trim().is_empty() {
                    map.insert(conversation_id.to_string(), workspace.to_string());
                }
            }
        }

        let last_path = self.base_dir.join("cache").join("last_conversations.json");
        if let Ok(content) = fs::read_to_string(last_path) {
            if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&content) {
                for (workspace, conversation_id) in obj {
                    if let Some(conversation_id) = conversation_id.as_str() {
                        if !workspace.trim().is_empty() {
                            map.entry(conversation_id.to_string())
                                .or_insert(workspace);
                        }
                    }
                }
            }
        }

        map
    }

    fn summary_for(&self, path: &PathBuf, project_path: Option<String>) -> SessionSummary {
        let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        if let Some(mt) = mtime {
            if let Some((cached_mt, cached_project, cached)) =
                self.summary_cache.read().unwrap().get(path)
            {
                if *cached_mt == mt && *cached_project == project_path {
                    return cached.clone();
                }
            }
        }

        let conversation_id = Self::conversation_id_from_path(path)
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string());
        let messages = self.parse_transcript_file(path).unwrap_or_default();
        let file_mtime = mtime.map(DateTime::<Utc>::from);
        let started_at = messages
            .first()
            .and_then(|m| m.timestamp)
            .or(file_mtime);
        let updated_at = messages.last().and_then(|m| m.timestamp).or(file_mtime);
        let title = Self::title_from_messages(&messages)
            .or_else(|| project_path.clone())
            .unwrap_or_else(|| "Antigravity Session".to_string());

        let summary = SessionSummary {
            id: conversation_id,
            tool: ToolKind::Antigravity,
            title,
            project_path: project_path.clone(),
            started_at,
            updated_at,
            message_count: messages.len(),
            total_tokens: sum_message_tokens(&messages),
        };

        if let Some(mt) = mtime {
            self.summary_cache
                .write()
                .unwrap()
                .insert(path.clone(), (mt, project_path, summary.clone()));
        }

        summary
    }

    fn title_from_messages(messages: &[Message]) -> Option<String> {
        messages
            .iter()
            .find(|m| m.role == Role::User)
            .and_then(|m| {
                m.content.iter().find_map(|block| match block {
                    ContentBlock::Text { text } => {
                        let first_line = text.trim().lines().next().unwrap_or("").trim();
                        if first_line.is_empty() {
                            None
                        } else {
                            Some(Self::truncate_title(first_line))
                        }
                    }
                    _ => None,
                })
            })
    }

    fn truncate_title(text: &str) -> String {
        let truncated: String = text.chars().take(60).collect();
        if truncated.len() < text.len() {
            format!("{}...", truncated)
        } else {
            truncated
        }
    }

    fn parse_transcript_file(&self, path: &Path) -> AppResult<Vec<Message>> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse_transcript_content(&content))
    }

    fn parse_transcript_content(content: &str) -> Vec<Message> {
        content
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                if line.trim().is_empty() {
                    return None;
                }
                let value = serde_json::from_str::<Value>(line).ok()?;
                Self::parse_step(&value, index)
            })
            .collect()
    }

    fn parse_step(step: &Value, index: usize) -> Option<Message> {
        let step_type = step.get("type").and_then(|v| v.as_str())?;
        match step_type {
            "USER_INPUT" => Self::parse_user_input(step, index),
            "PLANNER_RESPONSE" => Self::parse_planner_response(step, index),
            // 内部状态快照和压缩上下文不属于用户可读对话，避免污染时间线。
            "CONVERSATION_HISTORY" | "CHECKPOINT" => None,
            _ => Self::parse_tool_step(step, index),
        }
    }

    fn parse_user_input(step: &Value, index: usize) -> Option<Message> {
        let raw = step.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let text = Self::extract_user_request(raw).unwrap_or_else(|| raw.trim().to_string());
        if text.is_empty() {
            return None;
        }

        Some(Message {
            id: Self::message_id(step, index),
            role: Role::User,
            content: vec![ContentBlock::Text { text }],
            timestamp: Self::timestamp(step),
            model: None,
            usage: None,
        })
    }

    fn parse_planner_response(step: &Value, index: usize) -> Option<Message> {
        let mut content = Vec::new();

        if let Some(thinking) = step.get("thinking").and_then(|v| v.as_str()) {
            if !thinking.trim().is_empty() {
                content.push(ContentBlock::Thinking {
                    text: thinking.to_string(),
                });
            }
        }

        if let Some(tool_calls) = step.get("tool_calls").and_then(|v| v.as_array()) {
            for (call_index, call) in tool_calls.iter().enumerate() {
                let tool_name = call
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let input = call.get("args").cloned().unwrap_or(Value::Null);
                let tool_id = call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        step.get("step_index")
                            .and_then(|v| v.as_i64())
                            .map(|n| format!("step-{}-tool-{}", n, call_index))
                    });
                content.push(ContentBlock::ToolUse {
                    tool_name,
                    tool_id,
                    input,
                    agent_id: None,
                });
            }
        }

        if let Some(text) = step.get("content").and_then(|v| v.as_str()) {
            if !text.trim().is_empty() {
                content.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }

        if content.is_empty() {
            return None;
        }

        Some(Message {
            id: Self::message_id(step, index),
            role: Role::Assistant,
            content,
            timestamp: Self::timestamp(step),
            model: step
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            usage: None,
        })
    }

    fn parse_tool_step(step: &Value, index: usize) -> Option<Message> {
        let step_type = step.get("type").and_then(|v| v.as_str()).unwrap_or("TOOL");
        let content = step
            .get("content")
            .map(Self::value_to_display_string)
            .unwrap_or_default();
        let status = step.get("status").and_then(|v| v.as_str()).unwrap_or("");

        if content.trim().is_empty() && status == "DONE" {
            return None;
        }

        let tool_id = step
            .get("step_index")
            .and_then(|v| v.as_i64())
            .map(|n| format!("step-{}", n));

        Some(Message {
            id: Self::message_id(step, index),
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult {
                tool_id,
                content: if content.trim().is_empty() {
                    format!("{} {}", step_type, status).trim().to_string()
                } else {
                    content
                },
                is_error: status != "DONE",
            }],
            timestamp: Self::timestamp(step),
            model: None,
            usage: None,
        })
    }

    /// Antigravity 会把原始用户输入包在 <USER_REQUEST> 中，同时附带本地时间、
    /// 设置变更等元数据。展示时只取用户真实请求，缺少标签再回退到完整 content。
    fn extract_user_request(raw: &str) -> Option<String> {
        let start_tag = "<USER_REQUEST>";
        let end_tag = "</USER_REQUEST>";
        let start = raw.find(start_tag)? + start_tag.len();
        let end = raw[start..].find(end_tag)? + start;
        let text = raw[start..end].trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }

    fn value_to_display_string(value: &Value) -> String {
        match value {
            Value::String(s) => s.to_string(),
            _ => serde_json::to_string_pretty(value).unwrap_or_default(),
        }
    }

    fn message_id(step: &Value, index: usize) -> String {
        step.get("step_index")
            .and_then(|v| v.as_i64())
            .map(|n| format!("antigravity-step-{}", n))
            .unwrap_or_else(|| format!("antigravity-msg-{}", index))
    }

    fn timestamp(step: &Value) -> Option<DateTime<Utc>> {
        step.get("created_at")
            .and_then(|v| v.as_str())
            .and_then(Self::parse_timestamp)
            .or_else(|| {
                step.get("timestamp")
                    .and_then(|v| v.as_i64())
                    .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
            })
    }

    fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn find_file_by_session_id(&self, session_id: &str) -> Option<PathBuf> {
        self.find_transcript_files()
            .into_iter()
            .find(|path| Self::conversation_id_from_path(path).as_deref() == Some(session_id))
    }

    fn file_content_matches(&self, path: &Path, query: &str) -> bool {
        let Ok(content) = fs::read_to_string(path) else {
            return false;
        };
        if !search::contains_ci(&content, query) {
            return false;
        }
        let messages = Self::parse_transcript_content(&content);
        search::messages_match_query(&messages, query)
    }
}

impl SessionProvider for AntigravityProvider {
    fn tool_kind(&self) -> ToolKind {
        ToolKind::Antigravity
    }

    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let workspace_map = self.workspace_map();
        let files = self.find_transcript_files();

        use rayon::prelude::*;
        let mut summaries: Vec<SessionSummary> = files
            .par_iter()
            .map(|path| {
                let project_path = Self::conversation_id_from_path(path)
                    .and_then(|id| workspace_map.get(&id).cloned());
                self.summary_for(path, project_path)
            })
            .collect();

        summaries.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.started_at);
            let b_time = b.updated_at.or(b.started_at);
            b_time.cmp(&a_time)
        });
        Ok(summaries)
    }

    fn get_session(&self, session_id: &str) -> AppResult<Session> {
        let path = self
            .find_file_by_session_id(session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
        let messages = self.parse_transcript_file(&path)?;
        let project_path = self.workspace_map().get(session_id).cloned();
        let summary = SessionSummary {
            id: session_id.to_string(),
            tool: ToolKind::Antigravity,
            title: Self::title_from_messages(&messages)
                .or_else(|| project_path.clone())
                .unwrap_or_else(|| "Antigravity Session".to_string()),
            project_path,
            started_at: messages.first().and_then(|m| m.timestamp),
            updated_at: messages.last().and_then(|m| m.timestamp),
            message_count: messages.len(),
            total_tokens: sum_message_tokens(&messages),
        };

        Ok(Session { summary, messages })
    }

    fn search_sessions(
        &self,
        query: &str,
        include_content: bool,
    ) -> AppResult<Vec<SessionSummary>> {
        let all = self.list_sessions()?;
        let mut path_map = HashMap::new();
        if include_content {
            for path in self.find_transcript_files() {
                if let Some(id) = Self::conversation_id_from_path(&path) {
                    path_map.insert(id, path);
                }
            }
        }

        use rayon::prelude::*;
        Ok(all
            .into_par_iter()
            .filter(|s| {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aicoder-session-viewer-antigravity-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_transcript(base: &Path, conversation_id: &str, content: &str) -> PathBuf {
        let path = base
            .join("brain")
            .join(conversation_id)
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn default_path_points_to_antigravity_cli_dir() {
        let p = AntigravityProvider::default_path().unwrap();
        assert!(p.ends_with(".gemini/antigravity-cli"));
    }

    #[test]
    fn parses_user_assistant_thinking_tool_calls_and_tool_results() {
        let content = r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","created_at":"2026-06-25T01:00:00Z","content":"<USER_REQUEST>\n帮我看代码\n</USER_REQUEST>\n<ADDITIONAL_METADATA>x</ADDITIONAL_METADATA>"}
{"step_index":1,"type":"CONVERSATION_HISTORY","status":"DONE","created_at":"2026-06-25T01:00:00Z"}
{"step_index":2,"type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-06-25T01:00:01Z","thinking":"先理解结构","tool_calls":[{"name":"view_file","args":{"path":"src/main.rs"}}],"content":"我来检查。"}
{"step_index":3,"type":"VIEW_FILE","status":"DONE","created_at":"2026-06-25T01:00:02Z","content":"fn main() {}"}
{"step_index":4,"type":"CHECKPOINT","status":"DONE","created_at":"2026-06-25T01:00:03Z","content":"skip"}
"#;

        let messages = AntigravityProvider::parse_transcript_content(content);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::User);
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::Text { text } if text == "帮我看代码"
        ));
        assert_eq!(messages[1].role, Role::Assistant);
        assert!(messages[1]
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::Thinking { text } if text == "先理解结构")));
        assert!(messages[1].content.iter().any(|block| matches!(
            block,
            ContentBlock::ToolUse { tool_name, input, .. }
                if tool_name == "view_file" && input.get("path").and_then(|v| v.as_str()) == Some("src/main.rs")
        )));
        assert_eq!(messages[2].role, Role::Tool);
        assert!(matches!(
            &messages[2].content[0],
            ContentBlock::ToolResult { content, is_error, .. }
                if content == "fn main() {}" && !is_error
        ));
    }

    #[test]
    fn user_input_without_request_tag_falls_back_to_raw_content() {
        let content = r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","content":"plain text"}"#;
        let messages = AntigravityProvider::parse_transcript_content(content);
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::Text { text } if text == "plain text"
        ));
    }

    #[test]
    fn list_sessions_uses_history_workspace_when_available_and_allows_missing_mapping() {
        let base = tmp_dir("workspace-map");
        write_transcript(
            &base,
            "conv-a",
            r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","created_at":"2026-06-25T01:00:00Z","content":"hello"}"#,
        );
        write_transcript(
            &base,
            "conv-b",
            r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","created_at":"2026-06-25T02:00:00Z","content":"world"}"#,
        );
        fs::write(
            base.join("history.jsonl"),
            r#"{"display":"hello","workspace":"/tmp/project-a","conversationId":"conv-a"}"#,
        )
        .unwrap();

        let provider = AntigravityProvider::new(Some(base.clone())).unwrap();
        let sessions = provider.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions
                .iter()
                .find(|s| s.id == "conv-a")
                .unwrap()
                .project_path
                .as_deref(),
            Some("/tmp/project-a")
        );
        assert!(sessions
            .iter()
            .find(|s| s.id == "conv-b")
            .unwrap()
            .project_path
            .is_none());

        let _ = fs::remove_dir_all(base);
    }
}
