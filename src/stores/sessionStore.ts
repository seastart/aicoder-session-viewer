import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Session, SessionSummary, ToolKind } from "../types";

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

  // 视图模式
  viewMode: ViewMode;
  expandedPaths: Set<string>;

  // 操作
  fetchSessions: () => Promise<void>;
  selectSession: (tool: ToolKind, sessionId: string) => Promise<void>;
  setToolFilter: (tool: ToolKind | null) => void;
  setSearchQuery: (query: string) => void;
  searchSessions: (query: string, tool?: ToolKind | null) => Promise<void>;
  setViewMode: (mode: ViewMode) => void;
  togglePathExpanded: (path: string) => void;
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
  viewMode: "flat",
  expandedPaths: new Set<string>(),

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
    get().fetchSessions();
  },

  setSearchQuery: (query) => {
    set({ searchQuery: query });
  },

  searchSessions: async (query, tool) => {
    if (!query.trim()) {
      get().fetchSessions();
      return;
    }
    set({ loading: true, error: null });
    try {
      const sessions: SessionSummary[] = await invoke("search_sessions", {
        query,
        tool: tool ?? null,
      });
      set({ sessions, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
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
}));
