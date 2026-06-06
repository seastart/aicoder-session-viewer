import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createPortal } from "react-dom";
import { clsx } from "clsx";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { User, Bot, Wrench, ChevronRight, ChevronDown, Brain, X } from "lucide-react";
import { format, type Locale as DateFnsLocale } from "date-fns";
import type { Message, ContentBlock, TokenUsage } from "../../types";
import { CodeBlock } from "./CodeBlock";
import { ToolUseBlock, ToolResultBlock } from "./ToolCallBlock";
import { useLocale } from "../../i18n";

interface Props {
  message: Message;
  /** 当前 session id，用于加载 subagent 对话 */
  sessionId?: string;
  /** 当前消息是否包含会话内搜索命中 */
  searchMatched?: boolean;
  /** 当前消息是否是正在查看的搜索命中 */
  activeSearchMatch?: boolean;
}

export function MessageBubble({
  message,
  sessionId,
  searchMatched = false,
  activeSearchMatch = false,
}: Props) {
  const { t, dateLocale } = useLocale();
  const messageTime = formatMessageTime(message.timestamp, dateLocale);

  // 渲染按「内容」而非「role」判定：工具结果在协议上被塞进 user 回合，
  // 但它是工具输出、不是人在说话。若一条 user/tool 回合不含任何人类输入
  // （文本/代码/图片）、只有工具结果，则视为「工具输出」回合，用中性样式而非 User。
  const hasHumanInput = message.content.some(
    (b) => b.type === "text" || b.type === "code" || b.type === "image",
  );
  const hasToolResult = message.content.some((b) => b.type === "tool_result");
  const isToolOutput =
    (message.role === "user" || message.role === "tool") &&
    !hasHumanInput &&
    hasToolResult;

  const isUser = message.role === "user" && !isToolOutput;
  const isSystem = message.role === "system";

  return (
    <div
      className={clsx(
        "flex gap-3 border-l-4 px-4 py-3 transition-colors",
        isUser ? "bg-user-bubble/30" : "",
        searchMatched
          ? "border-accent/70 bg-accent/10"
          : "border-transparent",
        activeSearchMatch
          ? "border-accent bg-accent/15 ring-1 ring-inset ring-accent/80"
          : "",
      )}
    >
      {/* 头像 */}
      <div
        className={clsx(
          "flex h-7 w-7 shrink-0 items-center justify-center rounded-full",
          isUser
            ? "bg-accent/20 text-accent"
            : isToolOutput || isSystem
              ? "bg-text-muted/20 text-text-muted"
              : "bg-success/20 text-success"
        )}
      >
        {isUser ? (
          <User size={14} />
        ) : isToolOutput ? (
          <Wrench size={14} />
        ) : (
          <Bot size={14} />
        )}
      </div>

      {/* 内容 */}
      <div className="min-w-0 flex-1">
        {/* 角色 + 模型 + 时间 */}
        <div className="mb-1 flex items-center gap-2 text-xs text-text-muted">
          <span className="font-medium">
            {isUser
              ? t.roleUser
              : isToolOutput
                ? t.roleToolResult
                : isSystem
                  ? t.roleSystem
                  : t.roleAssistant}
          </span>
          {message.model && (
            <span className="rounded bg-surface px-1.5 py-0.5 text-[10px]">
              {message.model}
            </span>
          )}
          {message.usage && (
            <span className="text-[10px]" title={formatUsageTooltip(message.usage)}>
              {message.usage.input_tokens != null && `↑${message.usage.input_tokens.toLocaleString()}`}
              {message.usage.output_tokens != null && ` ↓${message.usage.output_tokens.toLocaleString()}`}
            </span>
          )}
          {messageTime && (
            <time
              dateTime={message.timestamp ?? undefined}
              title={messageTime.full}
              className="ml-auto shrink-0 text-[10px] tabular-nums text-text-muted"
            >
              {messageTime.short}
            </time>
          )}
        </div>

        {/* 内容块 */}
        <div className="space-y-1">
          {message.content.map((block, i) => (
            <ContentBlockRenderer key={i} block={block} sessionId={sessionId} />
          ))}
        </div>
      </div>
    </div>
  );
}

function formatMessageTime(
  timestamp: string | null,
  dateLocale: DateFnsLocale,
): { short: string; full: string } | null {
  if (!timestamp) {
    return null;
  }

  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  return {
    short: format(date, "MM-dd HH:mm", { locale: dateLocale }),
    full: format(date, "yyyy-MM-dd HH:mm:ss", { locale: dateLocale }),
  };
}

function ContentBlockRenderer({ block, sessionId }: { block: ContentBlock; sessionId?: string }) {
  switch (block.type) {
    case "text":
      return <MarkdownContent text={block.text} />;
    case "code":
      return <CodeBlock code={block.code} language={block.language} />;
    case "tool_use":
      return <ToolUseBlock block={block} sessionId={sessionId} />;
    case "tool_result":
      return <ToolResultBlock block={block} />;
    case "thinking":
      return <ThinkingBlock text={block.text} />;
    case "image":
      return <ImageBlock source={block.source} mediaType={block.media_type} />;
    default:
      return null;
  }
}

/**
 * 是否疑似裸 base64 数据：足够长且仅由 base64 字符集组成。
 * 注意 base64 字符集本身包含 '/'，故不能用「是否含 '/'」来区分路径，
 * 必须用字符集判定（文件路径含 '.'、'-'、中文等非 base64 字符）。
 */
function isProbablyBase64(s: string): boolean {
  return s.length > 200 && /^[A-Za-z0-9+/=\s]+$/.test(s);
}

/**
 * 图片块：缩略展示 + 点击放大查看
 *
 * source 可能是：
 * - 完整 data URI（以 "data:" 开头）或 http(s) URL → 直接使用
 * - 本地文件路径（如 Codex 生成图片的绝对路径）→ webview 无法直接加载，
 *   调用后端 read_image_data_uri 读取并转成 data URI
 * - 裸 base64 数据 → 拼接 mediaType 构造 data URI
 */
function ImageBlock({ source, mediaType }: { source: string; mediaType?: string | null }) {
  const [zoomed, setZoomed] = useState(false);
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  // 解析 source 为可显示的 src
  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    setSrc(null);

    if (
      source.startsWith("data:") ||
      source.startsWith("http://") ||
      source.startsWith("https://")
    ) {
      setSrc(source);
    } else if (isProbablyBase64(source)) {
      // 裸 base64（优先于路径判定：base64 含 '/'，不能按路径处理）
      setSrc(`data:${mediaType ?? "image/png"};base64,${source}`);
    } else {
      // 其余视为本地文件路径，交后端读取为 data URI
      const path = source.startsWith("file://")
        ? source.slice("file://".length)
        : source;
      invoke<string | null>("read_image_data_uri", { path })
        .then((uri) => {
          if (cancelled) return;
          if (uri) setSrc(uri);
          else setFailed(true);
        })
        .catch(() => {
          if (!cancelled) setFailed(true);
        });
    }

    return () => {
      cancelled = true;
    };
  }, [source, mediaType]);

  // 打开放大态时禁用页面滚动并支持 Esc 关闭
  useEffect(() => {
    if (!zoomed) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setZoomed(false);
    };
    document.addEventListener("keydown", onKey);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = prevOverflow;
    };
  }, [zoomed]);

  if (failed) {
    return (
      <span className="text-xs italic text-text-muted">[图片无法加载]</span>
    );
  }
  if (!src) {
    return <span className="text-xs text-text-muted">加载图片…</span>;
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setZoomed(true)}
        className="block cursor-zoom-in rounded-lg transition-opacity hover:opacity-90 focus:outline-none focus:ring-2 focus:ring-accent"
        title="点击查看大图"
      >
        <img src={src} alt="attachment" className="max-w-md rounded-lg" />
      </button>
      {zoomed &&
        createPortal(
          <div
            className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/80 p-6"
            onClick={() => setZoomed(false)}
            role="dialog"
            aria-modal="true"
          >
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setZoomed(false);
              }}
              className="absolute right-4 top-4 rounded-full bg-black/50 p-2 text-white hover:bg-black/70"
              aria-label="关闭"
            >
              <X size={20} />
            </button>
            {/* 阻止冒泡：点击图片本身不关闭，仅点击空白区域才关闭 */}
            <img
              src={src}
              alt="attachment full size"
              className="max-h-full max-w-full cursor-zoom-out rounded-lg shadow-2xl"
              onClick={(e) => e.stopPropagation()}
            />
          </div>,
          document.body,
        )}
    </>
  );
}

/** Markdown 渲染，内嵌代码块使用 shiki 高亮 */
function MarkdownContent({ text }: { text: string }) {
  return (
    <div className="prose prose-invert prose-sm max-w-none
                    prose-p:my-1 prose-headings:my-2
                    prose-pre:my-2 prose-pre:p-0 prose-pre:bg-transparent
                    prose-code:text-accent prose-code:before:content-none prose-code:after:content-none
                    prose-a:text-accent prose-a:no-underline hover:prose-a:underline">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        children={text}
        components={{
          // 图片拦截：本地路径（如 Codex 生成图片）经后端读取为 data URI 显示
          img({ src }) {
            if (!src || typeof src !== "string") return null;
            return <ImageBlock source={src} />;
          },
          // 代码块拦截：提取语言并用 CodeBlock 渲染
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || "");
            const code = String(children).replace(/\n$/, "");

            // 判断是否是块级代码（有 language class 或包含换行）
            if (match || code.includes("\n")) {
              return <CodeBlock code={code} language={match?.[1]} />;
            }

            // 行内代码
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          },
        }}
      />
    </div>
  );
}

/** 思考过程（可折叠） */
function ThinkingBlock({ text }: { text: string }) {
  const { t } = useLocale();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="my-1.5 rounded-lg border border-purple-800/30 bg-thinking-bg">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-surface-hover rounded-lg"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <Brain size={14} className="text-purple-400" />
        <span className="text-xs text-purple-400">{t.thinkingProcess}</span>
      </button>

      {expanded && (
        <div className="border-t border-purple-800/30 px-3 py-2">
          <pre className="overflow-x-auto whitespace-pre-wrap text-xs text-text-secondary">
            {text}
          </pre>
        </div>
      )}
    </div>
  );
}

/** 构建单条消息的 usage tooltip，展示缓存命中等细节 */
function formatUsageTooltip(usage: TokenUsage): string {
  const parts: string[] = [];
  if (usage.input_tokens != null) {
    let inputStr = `输入: ${usage.input_tokens.toLocaleString()}`;
    const cacheDetails: string[] = [];
    if (usage.cache_read_tokens) {
      cacheDetails.push(`缓存命中: ${usage.cache_read_tokens.toLocaleString()}`);
    }
    if (usage.cache_creation_tokens) {
      cacheDetails.push(`新建缓存: ${usage.cache_creation_tokens.toLocaleString()}`);
    }
    if (cacheDetails.length > 0) {
      inputStr += ` (${cacheDetails.join(", ")})`;
    }
    parts.push(inputStr);
  }
  if (usage.output_tokens != null) {
    parts.push(`输出: ${usage.output_tokens.toLocaleString()}`);
  }
  return parts.join(" | ");
}
