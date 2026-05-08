/** 翻译表类型定义 */
export interface Locale {
  // 通用
  loading: string;
  all: string;

  // 侧边栏
  appTitle: string;
  searchPlaceholder: string;
  noSessions: string;

  // 聊天区
  selectSession: string;
  messageCount: (n: number) => string;
  tokenDetail: (input: number, output: number, cacheRead: number, cacheCreation: number) => string;
  sessionSearchPlaceholder: string;
  sessionSearchCount: (current: number, total: number) => string;
  clearSearch: string;
  previousMatch: string;
  nextMatch: string;
  scrollToTop: string;
  scrollToBottom: string;

  // 角色
  roleUser: string;
  roleSystem: string;
  roleAssistant: string;

  // 思考过程
  thinkingProcess: string;

  // 工具调用
  toolResult: string;
  errorResult: string;

  // 子代理
  subagentLoading: string;
  subagentLoadError: (err: string) => string;
  subagentEmpty: string;
  subagentPrompt: string;
  subagentAgent: string;
  subagentError: string;
  subagentResult: string;

  // 导出
  exportJsonl: string;
  exportMarkdown: string;
  exportSuccess: string;
  exportError: (err: string) => string;
  export: string;

  // 恢复会话
  resumeSession: string;
  resumeSuccess: string;
  resumeError: (err: string) => string;
  autoContinue: string;
  autoContinueTooltip: (time: string) => string;
  autoContinueError: (err: string) => string;

  // 项目视图
  viewFlat: string;
  viewGrouped: string;
  newSession: string;
  ungrouped: string;
  sessionCount: (n: number) => string;
}

/** 中文翻译 */
const zh: Locale = {
  loading: "加载中...",
  all: "全部",
  appTitle: "AICoder Session Viewer",
  searchPlaceholder: "搜索 session...",
  noSessions: "未找到 session",
  selectSession: "选择一个 session 查看对话",
  messageCount: (n) => `${n} 条消息`,
  tokenDetail: (input, output, cacheRead, cacheCreation) =>
    `输入: ${input.toLocaleString()}${cacheRead ? ` (缓存命中: ${cacheRead.toLocaleString()})` : ""}${cacheCreation ? ` (新建缓存: ${cacheCreation.toLocaleString()})` : ""} | 输出: ${output.toLocaleString()}`,
  sessionSearchPlaceholder: "搜索当前对话...",
  sessionSearchCount: (current, total) => `${current}/${total}`,
  clearSearch: "清空搜索",
  previousMatch: "上一个命中",
  nextMatch: "下一个命中",
  scrollToTop: "回到顶部",
  scrollToBottom: "跳到底部",
  roleUser: "User",
  roleSystem: "System",
  roleAssistant: "Assistant",
  thinkingProcess: "思考过程",
  toolResult: "工具结果",
  errorResult: "错误结果",
  subagentLoading: "加载子代理对话...",
  subagentLoadError: (err) => `加载失败: ${err}`,
  subagentEmpty: "无子代理对话内容",
  subagentPrompt: "Prompt",
  subagentAgent: "Agent",
  subagentError: "错误",
  subagentResult: "结果",

  // 导出
  exportJsonl: "导出 JSONL",
  exportMarkdown: "导出 Markdown",
  exportSuccess: "导出成功",
  exportError: (err) => `导出失败: ${err}`,
  export: "导出",

  // 恢复会话
  resumeSession: "恢复会话",
  resumeSuccess: "已在终端中打开",
  resumeError: (err) => `恢复失败: ${err}`,
  autoContinue: "定时继续",
  autoContinueTooltip: (time) =>
    `等待到 ${time}（含 5 分钟缓冲）后，再恢复会话并发送 continue；支持的 CLI 会自动启用无人值守权限模式`,
  autoContinueError: (err) => `定时继续失败: ${err}`,

  // 项目视图
  viewFlat: "列表视图",
  viewGrouped: "项目视图",
  newSession: "新建会话",
  ungrouped: "未分类",
  sessionCount: (n) => `${n} 个会话`,
};

export default zh;
