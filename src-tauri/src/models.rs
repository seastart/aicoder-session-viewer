use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// AI 编码工具种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    ClaudeCode,
    Codex,
    Gemini,
    OpenCode,
}

impl ToolKind {
    /// 从字符串解析工具类型
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude_code" | "claudecode" | "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "open_code" | "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
}

/// Session 摘要，用于侧边栏列表展示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub tool: ToolKind,
    pub title: String,
    pub project_path: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub message_count: usize,
}

/// 完整 Session，包含所有消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub summary: SessionSummary,
    pub messages: Vec<Message>,
}

/// 单条消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub timestamp: Option<DateTime<Utc>>,
    pub model: Option<String>,
    /// token 用量（输入/输出）
    pub usage: Option<TokenUsage>,
}

/// 消息角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// 内容块 — 所有工具的输出都归一化为这几种类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// 普通文本 / Markdown
    Text { text: String },
    /// 代码块（带语言标注）
    Code { language: Option<String>, code: String },
    /// 工具调用
    ToolUse {
        tool_name: String,
        tool_id: Option<String>,
        input: serde_json::Value,
    },
    /// 工具调用结果
    ToolResult {
        tool_id: Option<String>,
        content: String,
        is_error: bool,
    },
    /// 思考过程（Claude extended thinking 等）
    Thinking { text: String },
    /// 图片（base64 或路径）
    Image {
        source: String,
        media_type: Option<String>,
    },
}

/// Token 用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
}
