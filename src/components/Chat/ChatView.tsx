import { useRef, useEffect, useMemo, useState } from "react";
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
  Eye,
  ChevronDown,
  ChevronUp,
  FileText,
  FileJson,
  FileCode,
  Zap,
  Search,
  X,
  ArrowDownToLine,
  ArrowUpToLine,
} from "lucide-react";
import { format } from "date-fns";
import { useLocale } from "../../i18n";
import {
  findSessionSearchMatches,
  isSessionSearchShortcut,
} from "../../utils/sessionSearch";
import { useAltKeyPressed } from "../../hooks/useAltKeyPressed";
import { YoloHint } from "../common/YoloHint";

const SCHEDULED_CONTINUE_BUFFER_MS = 5 * 60 * 1000;

export function ChatView() {
  const { currentSession, loading } = useSessionStore();
  const { t, dateLocale } = useLocale();
  const scrollRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const messageRefs = useRef<Array<HTMLDivElement | null>>([]);
  const [exportOpen, setExportOpen] = useState(false);
  const [resumeMenuOpen, setResumeMenuOpen] = useState(false);
  const [sessionSearchQuery, setSessionSearchQuery] = useState("");
  const [activeMatchIndex, setActiveMatchIndex] = useState(0);
  const altPressed = useAltKeyPressed();
  const sessionMessages = currentSession?.messages ?? [];
  const currentSessionId = currentSession?.summary.id;
  const searchMatches = useMemo(
    () => findSessionSearchMatches(sessionMessages, sessionSearchQuery),
    [sessionMessages, sessionSearchQuery],
  );
  const activeMatch = searchMatches[activeMatchIndex] ?? null;
  const matchedMessageIndexes = useMemo(
    () => new Set(searchMatches.map((match) => match.messageIndex)),
    [searchMatches],
  );

  // 切换 session 时滚动到顶部
  useEffect(() => {
    scrollRef.current?.scrollTo(0, 0);
    setSessionSearchQuery("");
    setActiveMatchIndex(0);
  }, [currentSessionId]);

  useEffect(() => {
    setActiveMatchIndex(0);
  }, [sessionSearchQuery, currentSessionId]);

  useEffect(() => {
    if (!activeMatch) {
      return;
    }
    messageRefs.current[activeMatch.messageIndex]?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
  }, [activeMatch]);

  useEffect(() => {
    if (!currentSessionId) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isSessionSearchShortcut(event)) {
        return;
      }

      event.preventDefault();
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [currentSessionId]);

  // 点击外部关闭导出菜单
  useEffect(() => {
    if (!exportOpen) return;
    const handleClick = () => setExportOpen(false);
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, [exportOpen]);

  // 点击外部关闭右键恢复菜单
  useEffect(() => {
    if (!resumeMenuOpen) return;
    const close = () => setResumeMenuOpen(false);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [resumeMenuOpen]);

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

  // OpenCode 暂不支持 bypass 开关，UI 上需要做友好降级
  const yoloSupported = summary.tool !== "open_code";

  // 汇总 session 级别的 token 用量
  const totalUsage = aggregateTokenUsage(messages);
  const autoContinueAt = inferAutoContinueTime(messages);
  const showScheduledContinue = autoContinueAt !== null;
  const autoContinueAtText =
    autoContinueAt === null
      ? ""
      : format(new Date(autoContinueAt), "yyyy-MM-dd HH:mm", {
          locale: dateLocale,
        });

  /** 恢复会话；bypass=true 时以 YOLO 模式启动 */
  const handleResume = async (opts: { bypass: boolean } = { bypass: false }) => {
    // OpenCode 不支持 bypass：即使用户按了 Alt 也只能按普通模式启动
    const effectiveBypass = opts.bypass && yoloSupported;
    try {
      await invoke("resume_session", {
        tool: summary.tool,
        sessionId: summary.id,
        projectPath: summary.project_path,
        bypassPermissions: effectiveBypass,
      });
    } catch (err) {
      console.error("Resume failed:", err);
    }
  };

  /** 等待到推断出的 reset 时间，再恢复会话并发送 continue */
  const handleAutoContinue = async () => {
    if (autoContinueAt === null) {
      return;
    }

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

  /** 导出为 HTML 网页文件（自包含：Markdown 渲染 + 图片 base64 内嵌） */
  const handleExportHtml = async () => {
    const fileName = buildExportFileName(summary, "html");
    const path = await save({
      defaultPath: fileName,
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    try {
      await invoke("export_session_html", {
        tool: summary.tool,
        sessionId: summary.id,
        savePath: path,
      });
    } catch (err) {
      console.error("Export HTML failed:", err);
    }
  };

  /** 网页查看：后端生成自包含 HTML 并用默认浏览器打开 */
  const handleOpenInBrowser = async () => {
    try {
      await invoke("open_session_in_browser", {
        tool: summary.tool,
        sessionId: summary.id,
      });
    } catch (err) {
      console.error("Open in browser failed:", err);
    }
  };

  const scrollToTop = () => {
    scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" });
  };

  const scrollToBottom = () => {
    const scrollEl = scrollRef.current;
    if (!scrollEl) {
      return;
    }
    scrollEl.scrollTo({ top: scrollEl.scrollHeight, behavior: "smooth" });
  };

  const jumpSearchMatch = (direction: 1 | -1) => {
    if (searchMatches.length === 0) {
      return;
    }

    // 命中列表是循环导航：长 session 中连续按按钮时不需要手动回到开头。
    setActiveMatchIndex((current) =>
      (current + direction + searchMatches.length) % searchMatches.length,
    );
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
            <div className="relative shrink-0">
              <button
                onClick={(e) => handleResume({ bypass: e.altKey })}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setResumeMenuOpen((open) => !open);
                }}
                className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
                title={yoloSupported ? `${t.resumeSession} · ${t.yoloAltHint}` : t.resumeSession}
              >
                <Play size={12} />
                <span className="hidden whitespace-nowrap md:inline">
                  {t.resumeSession}
                </span>
                {altPressed && yoloSupported && <YoloHint />}
              </button>

              {resumeMenuOpen && (
                <div
                  className="absolute right-0 top-full mt-1 z-20 w-48 rounded-md border border-border bg-surface shadow-lg"
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    onClick={() => {
                      setResumeMenuOpen(false);
                      if (yoloSupported) handleResume({ bypass: true });
                    }}
                    disabled={!yoloSupported}
                    className="flex w-full items-center gap-2 px-3 py-2 text-xs text-text-primary hover:bg-surface-hover transition-colors disabled:cursor-not-allowed disabled:text-text-muted disabled:hover:bg-transparent"
                    title={yoloSupported ? undefined : t.yoloUnsupportedOpenCode}
                  >
                    <Zap size={12} />
                    {t.yoloResumeMenuItem}
                  </button>
                </div>
              )}
            </div>

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

            {/* 在浏览器中查看 */}
            <button
              onClick={handleOpenInBrowser}
              className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
              title={t.viewInBrowser}
            >
              <Eye size={12} />
              <span className="hidden whitespace-nowrap md:inline">
                {t.viewInBrowser}
              </span>
            </button>

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
                  <button
                    onClick={handleExportHtml}
                    className="flex w-full items-center gap-2 px-3 py-2 text-xs text-text-primary hover:bg-surface-hover transition-colors"
                  >
                    <FileCode size={14} />
                    {t.exportHtml}
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

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <div className="relative min-w-[220px] flex-1 sm:max-w-md">
            <Search
              size={14}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted"
            />
            <input
              ref={searchInputRef}
              type="text"
              value={sessionSearchQuery}
              onChange={(event) => setSessionSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") {
                  return;
                }
                event.preventDefault();
                jumpSearchMatch(event.shiftKey ? -1 : 1);
              }}
              placeholder={t.sessionSearchPlaceholder}
              className="h-8 w-full rounded-md border border-border bg-surface py-1 pl-8 pr-8 text-xs text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
            />
            {sessionSearchQuery && (
              <button
                type="button"
                onClick={() => setSessionSearchQuery("")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary"
                title={t.clearSearch}
              >
                <X size={14} />
              </button>
            )}
          </div>

          <span className="w-16 text-center text-xs text-text-muted">
            {sessionSearchQuery.trim()
              ? t.sessionSearchCount(
                  searchMatches.length === 0 ? 0 : activeMatchIndex + 1,
                  searchMatches.length,
                )
              : ""}
          </span>

          <button
            type="button"
            onClick={() => jumpSearchMatch(-1)}
            disabled={searchMatches.length === 0}
            className="rounded p-1.5 text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40"
            title={t.previousMatch}
          >
            <ChevronUp size={14} />
          </button>
          <button
            type="button"
            onClick={() => jumpSearchMatch(1)}
            disabled={searchMatches.length === 0}
            className="rounded p-1.5 text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40"
            title={t.nextMatch}
          >
            <ChevronDown size={14} />
          </button>
          <div className="h-5 w-px bg-border" />
          <button
            type="button"
            onClick={scrollToTop}
            className="rounded p-1.5 text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
            title={t.scrollToTop}
          >
            <ArrowUpToLine size={14} />
          </button>
          <button
            type="button"
            onClick={scrollToBottom}
            className="rounded p-1.5 text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
            title={t.scrollToBottom}
          >
            <ArrowDownToLine size={14} />
          </button>
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
              <div
                key={msg.id || i}
                ref={(element) => {
                  messageRefs.current[i] = element;
                }}
                className="scroll-mt-20"
              >
                <MessageBubble
                  message={msg}
                  sessionId={summary.id}
                  searchMatched={matchedMessageIndexes.has(i)}
                  activeSearchMatch={activeMatch?.messageIndex === i}
                />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function buildExportFileName(
  summary: SessionSummary,
  extension: "jsonl" | "md" | "html",
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
    case "antigravity":
      return "antigravity";
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
 * 从最近一条可解析的 rate limit 消息中推断恢复时间。
 *
 * 第一性原理上，是否展示“定时自动恢复”取决于：
 * 1. 会话里是否真的出现过可解析 reset 时间的限额消息；
 * 2. 这条消息对应的恢复时间现在是否还没过。
 *
 * 因此这里不再依赖“最近 1 小时”这种与业务目标弱相关的代理条件，
 * 而是直接基于最后一条 quota 消息本身来计算。
 */
function inferAutoContinueTime(messages: Message[], now = new Date()): number | null {
  const rateLimitInfo = findLatestRateLimitInfo(messages);
  if (!rateLimitInfo) {
    return null;
  }

  const continueAt = withScheduledContinueBuffer(
    nextLocalOccurrence(
      rateLimitInfo.reset.hour24,
      rateLimitInfo.reset.minute,
      rateLimitInfo.occurredAt ?? now,
    ),
  ).getTime();

  // 有原始时间戳时，若该次限额对应的恢复点已经过去，就不再展示定时按钮，
  // 避免把几天前的旧 quota 错误映射成“下一次同钟点”的未来任务。
  if (rateLimitInfo.occurredAt && continueAt <= now.getTime()) {
    return null;
  }

  return continueAt;
}

function findLatestRateLimitInfo(
  messages: Message[],
): {
  reset: { hour24: number; minute: number };
  occurredAt: Date | null;
} | null {
  for (let messageIndex = messages.length - 1; messageIndex >= 0; messageIndex -= 1) {
    const message = messages[messageIndex];
    for (let blockIndex = message.content.length - 1; blockIndex >= 0; blockIndex -= 1) {
      const text = blockToSearchableText(message.content[blockIndex]);
      if (!text) {
        continue;
      }

      const parsed = extractResetClock(text);
      if (parsed && containsRateLimitSignal(text)) {
        return {
          reset: parsed,
          occurredAt: parseMessageTimestamp(message.timestamp),
        };
      }
    }
  }

  return null;
}

function parseMessageTimestamp(timestamp: string | null | undefined): Date | null {
  if (!timestamp) {
    return null;
  }

  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) {
    return null;
  }

  return parsed;
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
