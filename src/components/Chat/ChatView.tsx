import { useRef, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useSessionStore } from "../../stores/sessionStore";
import { TOOL_CONFIG } from "../../types";
import { MessageBubble } from "./MessageBubble";
import {
  Folder,
  Clock,
  MessageSquare,
  Play,
  Download,
  ChevronDown,
  FileText,
  FileJson,
} from "lucide-react";
import { format } from "date-fns";
import { useLocale } from "../../i18n";

export function ChatView() {
  const { currentSession, loading } = useSessionStore();
  const { t, dateLocale } = useLocale();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [exportOpen, setExportOpen] = useState(false);

  // 切换 session 时滚动到顶部
  useEffect(() => {
    scrollRef.current?.scrollTo(0, 0);
  }, [currentSession?.summary.id]);

  // 点击外部关闭导出菜单
  useEffect(() => {
    if (!exportOpen) return;
    const handleClick = () => setExportOpen(false);
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, [exportOpen]);

  if (!currentSession) {
    return (
      <div className="flex h-full items-center justify-center text-text-muted">
        <div className="text-center">
          <MessageSquare size={48} className="mx-auto mb-4 opacity-30" />
          <p>{t.selectSession}</p>
        </div>
      </div>
    );
  }

  const { summary, messages } = currentSession;
  const config = TOOL_CONFIG[summary.tool];

  /** 恢复会话 */
  const handleResume = async () => {
    try {
      await invoke("resume_session", {
        tool: summary.tool,
        sessionId: summary.id,
        projectPath: summary.project_path,
      });
    } catch (err) {
      console.error("Resume failed:", err);
    }
  };

  /** 导出为 JSONL */
  const handleExportJsonl = async () => {
    const fileName = sanitizeFileName(summary.title) + ".jsonl";
    const path = await save({
      defaultPath: fileName,
      filters: [{ name: "JSONL", extensions: ["jsonl"] }],
    });
    if (!path) return;
    try {
      await invoke("export_session_jsonl", {
        tool: summary.tool,
        sessionId: summary.id,
        savePath: path,
      });
    } catch (err) {
      console.error("Export JSONL failed:", err);
    }
  };

  /** 导出为 Markdown */
  const handleExportMarkdown = async () => {
    const fileName = sanitizeFileName(summary.title) + ".md";
    const path = await save({
      defaultPath: fileName,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!path) return;
    try {
      await invoke("export_session_markdown", {
        tool: summary.tool,
        sessionId: summary.id,
        savePath: path,
      });
    } catch (err) {
      console.error("Export Markdown failed:", err);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Session 头部信息 */}
      <div className="shrink-0 border-b border-border px-4 py-3">
        <div className="flex items-center gap-2">
          <span
            className="rounded px-2 py-0.5 text-xs font-medium"
            style={{
              backgroundColor: config.bgColor,
              color: config.color,
            }}
          >
            {config.label}
          </span>
          <h2 className="truncate text-sm font-medium text-text-primary">
            {summary.title}
          </h2>

          {/* 操作按钮区域 */}
          <div className="ml-auto flex items-center gap-1">
            {/* 恢复会话按钮 */}
            <button
              onClick={handleResume}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-text-muted hover:bg-surface-hover hover:text-text-primary transition-colors"
              title={t.resumeSession}
            >
              <Play size={12} />
              <span className="hidden sm:inline">{t.resumeSession}</span>
            </button>

            {/* 导出下拉菜单 */}
            <div className="relative">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setExportOpen(!exportOpen);
                }}
                className="flex items-center gap-1 rounded px-2 py-1 text-xs text-text-muted hover:bg-surface-hover hover:text-text-primary transition-colors"
                title={t.export}
              >
                <Download size={12} />
                <ChevronDown size={10} />
              </button>

              {exportOpen && (
                <div className="absolute right-0 top-full mt-1 z-10 w-44 rounded-md border border-border bg-surface shadow-lg">
                  <button
                    onClick={handleExportJsonl}
                    className="flex w-full items-center gap-2 px-3 py-2 text-xs text-text-primary hover:bg-surface-hover transition-colors"
                  >
                    <FileJson size={14} />
                    {t.exportJsonl}
                  </button>
                  <button
                    onClick={handleExportMarkdown}
                    className="flex w-full items-center gap-2 px-3 py-2 text-xs text-text-primary hover:bg-surface-hover transition-colors"
                  >
                    <FileText size={14} />
                    {t.exportMarkdown}
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>

        <div className="mt-1 flex items-center gap-4 text-xs text-text-muted">
          {summary.project_path && (
            <span className="flex items-center gap-1">
              <Folder size={12} />
              {summary.project_path}
            </span>
          )}
          {summary.started_at && (
            <span className="flex items-center gap-1">
              <Clock size={12} />
              {format(new Date(summary.started_at), "yyyy-MM-dd HH:mm", {
                locale: dateLocale,
              })}
            </span>
          )}
          <span className="flex items-center gap-1">
            <MessageSquare size={12} />
            {t.messageCount(messages.length)}
          </span>
        </div>
      </div>

      {/* 消息列表 */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex h-full items-center justify-center text-text-muted">
            {t.loading}
          </div>
        ) : (
          <div className="divide-y divide-border/50">
            {messages.map((msg, i) => (
              <MessageBubble key={msg.id || i} message={msg} sessionId={summary.id} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** 清理文件名中的非法字符 */
function sanitizeFileName(name: string): string {
  return name
    .replace(/[/\\:*?"<>|]/g, "_")
    .replace(/\s+/g, "_")
    .slice(0, 50);
}
