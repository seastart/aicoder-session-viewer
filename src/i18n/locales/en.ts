import type { Locale } from "./zh";

/** English translations */
const en: Locale = {
  // 通用
  loading: "Loading...",
  all: "All",

  // 侧边栏
  appTitle: "AICoder Session Viewer",
  searchPlaceholder: "Search sessions...",
  noSessions: "No sessions found",

  // 聊天区
  selectSession: "Select a session to view conversation",
  messageCount: (n: number) => `${n} messages`,

  // 角色
  roleUser: "User",
  roleSystem: "System",
  roleAssistant: "Assistant",

  // 思考过程
  thinkingProcess: "Thinking",

  // 工具调用
  toolResult: "Tool Result",
  errorResult: "Error Result",

  // 子代理
  subagentLoading: "Loading subagent conversation...",
  subagentLoadError: (err: string) => `Load failed: ${err}`,
  subagentEmpty: "No subagent conversation",
  subagentPrompt: "Prompt",
  subagentAgent: "Agent",
  subagentError: "Error",
  subagentResult: "Result",

  // 导出
  exportJsonl: "Export JSONL",
  exportMarkdown: "Export Markdown",
  exportSuccess: "Exported successfully",
  exportError: (err: string) => `Export failed: ${err}`,
  export: "Export",

  // 恢复会话
  resumeSession: "Resume Session",
  resumeSuccess: "Opened in terminal",
  resumeError: (err: string) => `Resume failed: ${err}`,

  // 项目视图
  viewFlat: "List View",
  viewGrouped: "Project View",
  newSession: "New Session",
  ungrouped: "Ungrouped",
  sessionCount: (n: number) => `${n} sessions`,
};

export default en;
