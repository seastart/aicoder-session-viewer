import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { clsx } from "clsx";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { User, Bot, ChevronRight, ChevronDown, Brain, X } from "lucide-react";
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
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const messageTime = formatMessageTime(message.timestamp, dateLocale);

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
            : isSystem
              ? "bg-text-muted/20 text-text-muted"
              : "bg-success/20 text-success"
        )}
      >
        {isUser ? <User size={14} /> : <Bot size={14} />}
      </div>

      {/* 内容 */}
      <div className="min-w-0 flex-1">
        {/* 角色 + 模型 + 时间 */}
        <div className="mb-1 flex items-center gap-2 text-xs text-text-muted">
          <span className="font-medium">
            {isUser ? t.roleUser : isSystem ? t.roleSystem : t.roleAssistant}
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
 * 图片块：缩略展示 + 点击放大查看
 *
 * 后端可能传入完整 data URI（source 以 "data:" 开头），
 * 也可能只传 base64 数据，此时需要拼接 mediaType 自行构造 data URI。
 */
function ImageBlock({ source, mediaType }: { source: string; mediaType?: string | null }) {
  const [zoomed, setZoomed] = useState(false);
  const src = source.startsWith("data:")
    ? source
    : `data:${mediaType ?? "image/png"};base64,${source}`;

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
