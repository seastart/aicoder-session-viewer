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
    /// 最后活跃时间，用于排序（优先使用此字段，没有时回退到 started_at）
    pub updated_at: Option<DateTime<Utc>>,
    pub message_count: usize,
    /// session 级别的 token 汇总（输入+输出），用于列表展示
    pub total_tokens: Option<u64>,
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
    Code {
        language: Option<String>,
        code: String,
    },
    /// 工具调用
    ToolUse {
        tool_name: String,
        tool_id: Option<String>,
        input: serde_json::Value,
        /// Claude Code subagent 的 agent_id（仅 Agent 工具调用时有值）
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
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

/// 从消息列表汇总 token 总量（input + output）
pub fn sum_message_tokens(messages: &[Message]) -> Option<u64> {
    let mut total = 0u64;
    let mut has_any = false;
    for msg in messages {
        if let Some(usage) = &msg.usage {
            has_any = true;
            total += usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0);
        }
    }
    if has_any { Some(total) } else { None }
}

/// Token 用量
///
/// 注意：`input_tokens` 统一表示该轮 API 调用的实际总输入 token 数（含缓存部分），
/// 各 provider 在构造时负责归一化：
/// - Claude API: input_tokens = raw_input + cache_read + cache_creation
/// - Codex/OpenAI: input_tokens 已包含缓存，无需额外处理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
}
