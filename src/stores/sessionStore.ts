import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  ProviderConfig,
  ProviderConfigResponse,
  ProviderPaths,
  Session,
  SessionSummary,
  ToolKind,
  UpdateProviderConfigResponse,
} from "../types";

/** 视图模式：扁平列表 或 按项目分组 */
export type ViewMode = "flat" | "grouped";

interface SessionState {
  // 数据
  sessions: SessionSummary[];
  currentSession: Session | null;
  loading: boolean;
  error: string | null;

  // 过滤
  toolFilter: ToolKind | null;
  searchQuery: string;
  /** 最近一次搜索是否包含会话内容（Enter 深搜索）；切换工具筛选时按相同模式重放 */
  lastSearchIncludedContent: boolean;

  // 视图模式
  viewMode: ViewMode;
  expandedPaths: Set<string>;

  // Provider 路径配置
  providerConfig: ProviderConfig | null;
  providerDefaults: ProviderPaths | null;

  // 操作
  fetchSessions: () => Promise<void>;
  selectSession: (tool: ToolKind, sessionId: string) => Promise<void>;
  setToolFilter: (tool: ToolKind | null) => void;
  setSearchQuery: (query: string) => void;
  /** includeContent 为 true 时做会话内容全文搜索（开销大，由 Enter 显式触发） */
  searchSessions: (
    query: string,
    tool?: ToolKind | null,
    includeContent?: boolean
  ) => Promise<void>;
  setViewMode: (mode: ViewMode) => void;
  togglePathExpanded: (path: string) => void;
  loadProviderConfig: () => Promise<void>;
  saveProviderConfig: (config: ProviderConfig) => Promise<string[]>;
}

// 递增的请求 ID，用于防止异步竞态（旧请求的结果覆盖新请求）
let fetchRequestId = 0;

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  currentSession: null,
  loading: false,
  error: null,
  toolFilter: null,
  searchQuery: "",
  lastSearchIncludedContent: false,
  viewMode: "grouped",
  expandedPaths: new Set<string>(),
  providerConfig: null,
  providerDefaults: null,

  fetchSessions: async () => {
    const requestId = ++fetchRequestId;
    set({ loading: true, error: null });
    try {
      const { toolFilter } = get();
      let sessions: SessionSummary[];
      if (toolFilter) {
        sessions = await invoke("list_sessions", { tool: toolFilter });
      } else {
        sessions = await invoke("list_all_sessions");
      }
      // 仅当这是最新一次请求时才更新状态，防止旧结果覆盖
      if (requestId === fetchRequestId) {
        set({ sessions, loading: false });
      }
    } catch (e) {
      if (requestId === fetchRequestId) {
        // 出错时清空列表，避免展示不属于当前 filter 的旧数据
        set({ sessions: [], error: String(e), loading: false });
      }
    }
  },

  selectSession: async (tool, sessionId) => {
    set({ loading: true, error: null });
    try {
      const session: Session = await invoke("get_session", {
        tool,
        sessionId,
      });
      set({ currentSession: session, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  setToolFilter: (tool) => {
    // 切换 filter 时立即清空旧列表，避免展示不属于当前 filter 的数据
    set({ toolFilter: tool, sessions: [] });
    const { searchQuery, lastSearchIncludedContent } = get();
    if (searchQuery.trim()) {
      // 有搜索关键词时按原模式（浅/深）重放搜索，保持过滤结果一致
      get().searchSessions(searchQuery, tool, lastSearchIncludedContent);
    } else {
      get().fetchSessions();
    }
  },

  setSearchQuery: (query) => {
    set({ searchQuery: query });
  },

  searchSessions: async (query, tool, includeContent = false) => {
    if (!query.trim()) {
      get().fetchSessions();
      return;
    }
    // 复用 fetchRequestId 防竞态：深度搜索耗时较长，
    // 防止其过期结果覆盖后续更新的实时搜索/列表请求
    const requestId = ++fetchRequestId;
    // 记录本次搜索模式，供切换工具筛选时按相同模式重放
    set({ loading: true, error: null, lastSearchIncludedContent: includeContent });
    try {
      const sessions: SessionSummary[] = await invoke("search_sessions", {
        query,
        tool: tool ?? null,
        includeContent,
      });
      if (requestId === fetchRequestId) {
        set({ sessions, loading: false });
      }
    } catch (e) {
      if (requestId === fetchRequestId) {
        set({ error: String(e), loading: false });
      }
    }
  },

  setViewMode: (mode) => {
    set({ viewMode: mode });
  },

  togglePathExpanded: (path) => {
    set((state) => {
      const next = new Set(state.expandedPaths);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return { expandedPaths: next };
    });
  },

  loadProviderConfig: async () => {
    try {
      const resp: ProviderConfigResponse = await invoke("get_provider_config");
      set({
        providerConfig: resp.config,
        providerDefaults: resp.defaults,
      });
    } catch (e) {
      console.error("[store] loadProviderConfig 失败:", e);
    }
  },

  saveProviderConfig: async (config) => {
    const resp: UpdateProviderConfigResponse = await invoke(
      "update_provider_config",
      { config }
    );
    // 保存成功后立即刷新本地副本 + session 列表
    set({ providerConfig: config });
    await get().fetchSessions();
    return resp.warnings;
  },
}));
