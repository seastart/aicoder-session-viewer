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
};

export default zh;
