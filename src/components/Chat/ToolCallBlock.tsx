import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronRight,
  ChevronDown,
  Wrench,
  AlertCircle,
  Bot,
  Loader2,
} from "lucide-react";
import { clsx } from "clsx";
import type { ContentBlock, Message } from "../../types";
import { useLocale } from "../../i18n";

interface ToolUseProps {
  block: Extract<ContentBlock, { type: "tool_use" }>;
  /** 当前 session 的 id，用于加载 subagent 对话 */
  sessionId?: string;
}

export function ToolUseBlock({ block, sessionId }: ToolUseProps) {
  const [expanded, setExpanded] = useState(false);
  const isAgent = block.tool_name === "Agent" && !!block.agent_id;

  return (
    <div className="my-1.5 rounded-lg border border-border bg-tool-bg">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-surface-hover rounded-lg"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {isAgent ? (
          <Bot size={14} className="text-purple-400" />
        ) : (
          <Wrench size={14} className="text-accent" />
        )}
        <span className={clsx("font-medium", isAgent ? "text-purple-400" : "text-accent")}>
          {isAgent ? `Agent` : block.tool_name}
        </span>
        {/* Agent 调用时显示 description */}
        {isAgent && typeof block.input === "object" && block.input !== null && (
          <span className="text-xs text-text-muted truncate">
            {(block.input as Record<string, unknown>).description as string}
          </span>
        )}
      </button>

      {expanded && (
        <div className="border-t border-border">
          {isAgent && sessionId ? (
            <SubagentConversation sessionId={sessionId} agentId={block.agent_id!} />
          ) : (
            <div className="px-3 py-2">
              <pre className="overflow-x-auto text-xs text-text-secondary">
                {typeof block.input === "string"
                  ? block.input
                  : JSON.stringify(block.input, null, 2)}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** 懒加载 subagent 对话 */
function SubagentConversation({
  sessionId,
  agentId,
}: {
  sessionId: string;
  agentId: string;
}) {
  const [messages, setMessages] = useState<Message[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 首次渲染时自动加载
  if (messages === null && !loading && !error) {
    setLoading(true);
    invoke<Message[]>("get_subagent_messages", {
      sessionId,
      agentId,
    })
      .then((msgs) => {
        setMessages(msgs);
        setLoading(false);
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  }

  const { t } = useLocale();

  if (loading) {
    return (
      <div className="flex items-center gap-2 px-3 py-3 text-xs text-text-muted">
        <Loader2 size={14} className="animate-spin" />
        {t.subagentLoading}
      </div>
    );
  }

  if (error) {
    return (
      <div className="px-3 py-2 text-xs text-error">
        {t.subagentLoadError(error)}
      </div>
    );
  }

  if (!messages || messages.length === 0) {
    return (
      <div className="px-3 py-2 text-xs text-text-muted">
        {t.subagentEmpty}
      </div>
    );
  }

  return (
    <div className="max-h-96 overflow-y-auto border-l-2 border-purple-800/30 ml-2">
      {messages.map((msg, i) => (
        <SubagentMessage key={i} message={msg} />
      ))}
    </div>
  );
}

/** 简化版的 subagent 消息渲染 */
function SubagentMessage({ message }: { message: Message }) {
  const { t } = useLocale();
  const isUser = message.role === "user";

  return (
    <div className={clsx("px-3 py-2 text-xs", isUser ? "bg-user-bubble/10" : "")}>
      <div className="flex items-center gap-1.5 mb-0.5 text-text-muted">
        <span className="font-medium text-[10px]">
          {isUser ? t.subagentPrompt : t.subagentAgent}
        </span>
        {message.model && (
          <span className="rounded bg-surface px-1 py-0.5 text-[9px]">
            {message.model}
          </span>
        )}
      </div>
      <div className="text-text-secondary space-y-1">
        {message.content.map((block, i) => (
          <SubagentContentBlock key={i} block={block} />
        ))}
      </div>
    </div>
  );
}

/** 子代理内容块的简化渲染 */
function SubagentContentBlock({ block }: { block: ContentBlock }) {
  const { t } = useLocale();
  const [expanded, setExpanded] = useState(false);

  switch (block.type) {
    case "text":
      return (
        <pre className="whitespace-pre-wrap text-xs">{block.text}</pre>
      );
    case "tool_use":
      return (
        <div className="rounded border border-border/50 bg-surface/30">
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex items-center gap-1.5 px-2 py-1 text-[10px] text-text-muted w-full text-left hover:bg-surface-hover rounded"
          >
            {expanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
            <Wrench size={10} />
            <span>{block.tool_name}</span>
          </button>
          {expanded && (
            <pre className="px-2 py-1 text-[10px] text-text-muted border-t border-border/30 overflow-x-auto">
              {typeof block.input === "string"
                ? block.input
                : JSON.stringify(block.input, null, 2)}
            </pre>
          )}
        </div>
      );
    case "tool_result": {
      const preview = block.content.slice(0, 200);
      return (
        <div className="rounded border border-border/50 bg-surface/30">
          <button
            onClick={() => setExpanded(!expanded)}
            className={clsx(
              "flex items-center gap-1.5 px-2 py-1 text-[10px] w-full text-left hover:bg-surface-hover rounded",
              block.is_error ? "text-error" : "text-text-muted"
            )}
          >
            {expanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
            <span>{block.is_error ? t.subagentError : t.subagentResult}</span>
            {!expanded && (
              <span className="truncate text-text-muted ml-1">
                {preview}{preview.length < block.content.length ? "..." : ""}
              </span>
            )}
          </button>
          {expanded && (
            <pre className="px-2 py-1 text-[10px] text-text-muted border-t border-border/30 overflow-x-auto whitespace-pre-wrap max-h-60 overflow-y-auto">
              {block.content}
            </pre>
          )}
        </div>
      );
    }
    default:
      return null;
  }
}

interface ToolResultProps {
  block: Extract<ContentBlock, { type: "tool_result" }>;
}

export function ToolResultBlock({ block }: ToolResultProps) {
  const { t } = useLocale();
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      className={clsx(
        "my-1.5 rounded-lg border",
        block.is_error
          ? "border-error/30 bg-error/5"
          : "border-border bg-tool-bg"
      )}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-surface-hover rounded-lg"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {block.is_error && <AlertCircle size={14} className="text-error" />}
        <span
          className={clsx(
            "text-xs min-w-0 truncate",
            block.is_error ? "text-error" : "text-text-muted"
          )}
        >
          {block.is_error ? t.errorResult : t.toolResult}
          {!expanded && (
            <span className="ml-2 text-text-muted">
              {block.content.slice(0, 120)}
              {block.content.length > 120 ? "…" : ""}
            </span>
          )}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2">
          <pre className="overflow-x-auto whitespace-pre-wrap text-xs text-text-secondary">
            {block.content}
          </pre>
        </div>
      )}
    </div>
  );
}
