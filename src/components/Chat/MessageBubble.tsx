import { useState } from "react";
import { clsx } from "clsx";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { User, Bot, ChevronRight, ChevronDown, Brain } from "lucide-react";
import type { Message, ContentBlock, TokenUsage } from "../../types";
import { CodeBlock } from "./CodeBlock";
import { ToolUseBlock, ToolResultBlock } from "./ToolCallBlock";
import { useLocale } from "../../i18n";

interface Props {
  message: Message;
  /** 当前 session id，用于加载 subagent 对话 */
  sessionId?: string;
}

export function MessageBubble({ message, sessionId }: Props) {
  const { t } = useLocale();
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div
      className={clsx("flex gap-3 px-4 py-3", isUser ? "bg-user-bubble/30" : "")}
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
      return (
        <img
          src={
            block.source.startsWith("data:")
              ? block.source
              : `data:${block.media_type ?? "image/png"};base64,${block.source}`
          }
          alt="AI generated"
          className="max-w-md rounded-lg"
        />
      );
    default:
      return null;
  }
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
