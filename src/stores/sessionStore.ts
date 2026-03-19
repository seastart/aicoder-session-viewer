import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Session, SessionSummary, ToolKind } from "../types";

interface SessionState {
  // 数据
  sessions: SessionSummary[];
  currentSession: Session | null;
  loading: boolean;
  error: string | null;

  // 过滤
  toolFilter: ToolKind | null;
  searchQuery: string;

  // 操作
  fetchSessions: () => Promise<void>;
  selectSession: (tool: ToolKind, sessionId: string) => Promise<void>;
  setToolFilter: (tool: ToolKind | null) => void;
  setSearchQuery: (query: string) => void;
  searchSessions: (query: string, tool?: ToolKind | null) => Promise<void>;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  currentSession: null,
  loading: false,
  error: null,
  toolFilter: null,
  searchQuery: "",

  fetchSessions: async () => {
    set({ loading: true, error: null });
    try {
      const { toolFilter } = get();
      let sessions: SessionSummary[];
      if (toolFilter) {
        sessions = await invoke("list_sessions", { tool: toolFilter });
      } else {
        sessions = await invoke("list_all_sessions");
      }
      set({ sessions, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
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
    set({ toolFilter: tool });
    // 重新获取 session 列表
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
}));
