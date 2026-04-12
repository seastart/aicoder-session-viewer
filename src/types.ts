/** AI 编码工具类型（对应 Rust ToolKind） */
export type ToolKind = "claude_code" | "codex" | "gemini" | "open_code";

/** Session 摘要，侧边栏列表使用 */
export interface SessionSummary {
  id: string;
  tool: ToolKind;
  title: string;
  project_path: string | null;
  started_at: string | null;
  /** 最后活跃时间 */
  updated_at: string | null;
  message_count: number;
  /** session 级别的 token 汇总（输入+输出） */
  total_tokens: number | null;
}

/** 完整 Session */
export interface Session {
  summary: SessionSummary;
  messages: Message[];
}

/** 单条消息 */
export interface Message {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: ContentBlock[];
  timestamp: string | null;
  model: string | null;
  usage: TokenUsage | null;
}

/** 内容块类型（tagged union） */
export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "code"; language: string | null; code: string }
  | { type: "tool_use"; tool_name: string; tool_id: string | null; input: unknown; agent_id?: string | null }
  | { type: "tool_result"; tool_id: string | null; content: string; is_error: boolean }
  | { type: "thinking"; text: string }
  | { type: "image"; source: string; media_type: string | null };

/** Token 用量
 *
 * input_tokens 统一为"该轮 API 调用的总输入 token 数"（含缓存部分），
 * 各 provider 在 Rust 侧已完成归一化。
 */
export interface TokenUsage {
  input_tokens: number | null;
  output_tokens: number | null;
  cache_read_tokens: number | null;
  cache_creation_tokens: number | null;
}

/** 工具显示配置 */
export const TOOL_CONFIG: Record<
  ToolKind,
  { label: string; color: string; bgColor: string }
> = {
  claude_code: { label: "Claude Code", color: "#d4a574", bgColor: "#3d2e1f" },
  codex: { label: "Codex", color: "#7ec8e3", bgColor: "#1f2d3d" },
  gemini: { label: "Gemini", color: "#8b9cf7", bgColor: "#252547" },
  open_code: { label: "OpenCode", color: "#a6e3a1", bgColor: "#1f3d2e" },
};
