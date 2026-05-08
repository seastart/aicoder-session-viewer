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
  tokenDetail: (input: number, output: number, cacheRead: number, cacheCreation: number) =>
    `Input: ${input.toLocaleString()}${cacheRead ? ` (cache hit: ${cacheRead.toLocaleString()})` : ""}${cacheCreation ? ` (cache write: ${cacheCreation.toLocaleString()})` : ""} | Output: ${output.toLocaleString()}`,
  sessionSearchPlaceholder: "Search current chat...",
  sessionSearchCount: (current: number, total: number) => `${current}/${total}`,
  clearSearch: "Clear search",
  previousMatch: "Previous match",
  nextMatch: "Next match",
  scrollToTop: "Back to top",
  scrollToBottom: "Jump to bottom",

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
  autoContinue: "Scheduled Continue",
  autoContinueTooltip: (time: string) =>
    `Wait until ${time} (including a 5-minute safety buffer), then resume the session and send continue; supported CLIs automatically enable unattended approval mode`,
  autoContinueError: (err: string) => `Auto continue failed: ${err}`,

  // 项目视图
  viewFlat: "List View",
  viewGrouped: "Project View",
  newSession: "New Session",
  ungrouped: "Ungrouped",
  sessionCount: (n: number) => `${n} sessions`,
};

export default en;
