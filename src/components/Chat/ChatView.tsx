import { useRef, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useSessionStore } from "../../stores/sessionStore";
import {
  TOOL_CONFIG,
  type Message,
  type SessionSummary,
  type ToolKind,
} from "../../types";
import { formatTokenCount } from "../../utils/format";
import { MessageBubble } from "./MessageBubble";
import {
  Folder,
  Clock,
  Clock3,
  MessageSquare,
  Play,
  Download,
  ChevronDown,
  FileText,
  FileJson,
  Zap,
} from "lucide-react";
import { format } from "date-fns";
import { useLocale } from "../../i18n";

const SCHEDULED_CONTINUE_BUFFER_MS = 5 * 60 * 1000;
const RATE_LIMIT_RECENCY_WINDOW_MS = 60 * 60 * 1000;

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

  // 汇总 session 级别的 token 用量
  const totalUsage = aggregateTokenUsage(messages);
  const showScheduledContinue = hasRecentRateLimitMessage(messages);
  const autoContinueAt = inferAutoContinueTime(messages);
  const autoContinueAtText = format(new Date(autoContinueAt), "yyyy-MM-dd HH:mm", {
    locale: dateLocale,
  });

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

  /** 等待到推断出的 reset 时间，再恢复会话并发送 continue */
  const handleAutoContinue = async () => {
    try {
      await invoke("resume_session_with_auto_continue", {
        tool: summary.tool,
        sessionId: summary.id,
        projectPath: summary.project_path,
        continueAtMs: autoContinueAt,
      });
    } catch (err) {
      console.error("Auto continue failed:", err);
      window.alert(t.autoContinueError(String(err)));
    }
  };

  /** 导出为 JSONL */
  const handleExportJsonl = async () => {
    const fileName = buildExportFileName(summary, "jsonl");
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
    const fileName = buildExportFileName(summary, "md");
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
        <div className="flex items-center gap-2 min-w-0">
          <span
            className="shrink-0 rounded px-2 py-0.5 text-xs font-medium whitespace-nowrap"
            style={{
              backgroundColor: config.bgColor,
              color: config.color,
            }}
          >
            {config.label}
          </span>
          <h2 className="min-w-0 flex-1 truncate text-sm font-medium text-text-primary">
            {summary.title}
          </h2>

          {/* 操作按钮区域 */}
          <div className="ml-auto flex shrink-0 items-center gap-1 whitespace-nowrap">
            {/* 恢复会话按钮 */}
            <button
              onClick={handleResume}
              className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
              title={t.resumeSession}
            >
              <Play size={12} />
              <span className="hidden whitespace-nowrap md:inline">
                {t.resumeSession}
              </span>
            </button>

            {showScheduledContinue && (
              <button
                onClick={handleAutoContinue}
                className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
                title={t.autoContinueTooltip(autoContinueAtText)}
              >
                <Clock3 size={12} />
                <span className="hidden whitespace-nowrap md:inline">
                  {t.autoContinue}
                </span>
              </button>
            )}

            {/* 导出下拉菜单 */}
            <div className="relative shrink-0">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setExportOpen(!exportOpen);
                }}
                className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
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
          {totalUsage.total > 0 && (
            <span
              className="flex items-center gap-1"
              title={t.tokenDetail(totalUsage.input, totalUsage.output, totalUsage.cacheRead, totalUsage.cacheCreation)}
            >
              <Zap size={12} />
              <span>↑{formatTokenCount(totalUsage.input)}</span>
              <span>↓{formatTokenCount(totalUsage.output)}</span>
            </span>
          )}
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

function buildExportFileName(
  summary: SessionSummary,
  extension: "jsonl" | "md",
): string {
  const parts = [toolSlug(summary.tool)];
  const timePart = formatExportTime(summary.started_at);
  if (timePart) {
    parts.push(timePart);
  }
  parts.push(shortSessionId(summary.id));
  return `${parts.join("-")}.${extension}`;
}

function toolSlug(tool: ToolKind): string {
  switch (tool) {
    case "claude_code":
      return "claude";
    case "codex":
      return "codex";
    case "gemini":
      return "gemini";
    case "open_code":
      return "opencode";
  }
}

function formatExportTime(startedAt: string | null): string | null {
  if (!startedAt) {
    return null;
  }

  const date = new Date(startedAt);
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  const pad = (value: number) => String(value).padStart(2, "0");
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
  ].join("") + `-${pad(date.getHours())}${pad(date.getMinutes())}`;
}

/** 仅保留稳定且简短的标识，避免导出名被长标题占满。 */
function shortSessionId(id: string): string {
  const safe = id.replace(/[^a-zA-Z0-9_-]/g, "");
  return safe.slice(0, 8) || "session";
}

/**
 * 优先从最近消息中推断 vendor 提示的 reset 时间；推断失败时回退到本地时区的下一个 01:00。
 * 为了避开供应商侧的时钟抖动、缓存和整点切换，真正执行时间会统一延后 5 分钟。
 *
 * 这样做的核心原则很简单：
 * 1. 如果日志里已经出现了精确 reset 时间，就直接复用；
 * 2. 如果没有，就采用用户最常见的 quota reset 点作为保守默认值。
 */
function inferAutoContinueTime(messages: Message[], now = new Date()): number {
  const parsed = inferResetClockFromMessages(messages);
  if (parsed) {
    return withScheduledContinueBuffer(
      nextLocalOccurrence(parsed.hour24, parsed.minute, now),
    ).getTime();
  }
  return withScheduledContinueBuffer(nextLocalOccurrence(1, 0, now)).getTime();
}

function hasRecentRateLimitMessage(messages: Message[], now = new Date()): boolean {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (!messageContainsRateLimit(message)) {
      continue;
    }

    if (!message.timestamp) {
      return true;
    }

    const timestamp = new Date(message.timestamp);
    if (Number.isNaN(timestamp.getTime())) {
      continue;
    }

    return now.getTime() - timestamp.getTime() <= RATE_LIMIT_RECENCY_WINDOW_MS;
  }

  return false;
}

function messageContainsRateLimit(message: Message): boolean {
  for (const block of message.content) {
    const text = blockToSearchableText(block);
    if (!text) {
      continue;
    }

    if (extractResetClock(text) && containsRateLimitSignal(text)) {
      return true;
    }
  }

  return false;
}

function inferResetClockFromMessages(
  messages: Message[],
): { hour24: number; minute: number } | null {
  for (let messageIndex = messages.length - 1; messageIndex >= 0; messageIndex -= 1) {
    const message = messages[messageIndex];
    for (let blockIndex = message.content.length - 1; blockIndex >= 0; blockIndex -= 1) {
      const text = blockToSearchableText(message.content[blockIndex]);
      if (!text) {
        continue;
      }

      const parsed = extractResetClock(text);
      if (parsed) {
        return parsed;
      }
    }
  }

  return null;
}

function extractResetClock(text: string): { hour24: number; minute: number } | null {
  const patterns = [
    /resets?\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)?(?:\s*\([^)]+\))?/i,
    /try again at\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b/i,
    /available again at\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b/i,
  ];

  for (const pattern of patterns) {
    const match = text.match(pattern);
    if (!match) {
      continue;
    }

    const hour = Number(match[1]);
    const minute = Number(match[2] ?? "0");
    if (!Number.isInteger(hour) || !Number.isInteger(minute)) {
      continue;
    }
    if (minute < 0 || minute > 59) {
      continue;
    }

    const hour24 = normalizeHour(hour, match[3]?.toLowerCase());
    if (hour24 == null) {
      continue;
    }

    return { hour24, minute };
  }

  return null;
}

function containsRateLimitSignal(text: string): boolean {
  return /usage limit|hit your limit|rate limit|quota|try again at|available again at|resets?/i.test(
    text,
  );
}

function blockToSearchableText(block: Message["content"][number]): string | null {
  switch (block.type) {
    case "text":
    case "thinking":
      return block.text;
    case "tool_result":
      return block.content;
    case "code":
      return block.code;
    case "image":
    case "tool_use":
      return null;
  }
}

function normalizeHour(hour: number, meridiem?: string): number | null {
  if (!Number.isInteger(hour)) {
    return null;
  }

  if (meridiem === "am") {
    return hour === 12 ? 0 : hour >= 0 && hour <= 11 ? hour : null;
  }

  if (meridiem === "pm") {
    if (hour < 1 || hour > 12) {
      return null;
    }
    return hour === 12 ? 12 : hour + 12;
  }

  return hour >= 0 && hour <= 23 ? hour : null;
}

function nextLocalOccurrence(hour: number, minute: number, now: Date): Date {
  const target = new Date(now);
  target.setSeconds(0, 0);
  target.setHours(hour, minute, 0, 0);

  if (target.getTime() <= now.getTime()) {
    target.setDate(target.getDate() + 1);
  }

  return target;
}

function withScheduledContinueBuffer(date: Date): Date {
  return new Date(date.getTime() + SCHEDULED_CONTINUE_BUFFER_MS);
}

/** 汇总所有消息的 token 用量
 *
 * input_tokens 已在后端归一化为"总输入"（含缓存），
 * 因此 total = input + output 即为真实 API 消耗。
 */
function aggregateTokenUsage(messages: Message[]): {
  input: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  total: number;
} {
  let input = 0;
  let output = 0;
  let cacheRead = 0;
  let cacheCreation = 0;

  for (const msg of messages) {
    if (!msg.usage) continue;
    input += msg.usage.input_tokens ?? 0;
    output += msg.usage.output_tokens ?? 0;
    cacheRead += msg.usage.cache_read_tokens ?? 0;
    cacheCreation += msg.usage.cache_creation_tokens ?? 0;
  }

  return { input, output, cacheRead, cacheCreation, total: input + output };
}

