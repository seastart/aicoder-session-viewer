import { useState } from "react";
import { ChevronRight, ChevronDown, Wrench, AlertCircle } from "lucide-react";
import { clsx } from "clsx";
import type { ContentBlock } from "../../types";

interface ToolUseProps {
  block: Extract<ContentBlock, { type: "tool_use" }>;
}

export function ToolUseBlock({ block }: ToolUseProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="my-1.5 rounded-lg border border-border bg-tool-bg">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-surface-hover rounded-lg"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <Wrench size={14} className="text-accent" />
        <span className="font-medium text-accent">{block.tool_name}</span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2">
          <pre className="overflow-x-auto text-xs text-text-secondary">
            {typeof block.input === "string"
              ? block.input
              : JSON.stringify(block.input, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

interface ToolResultProps {
  block: Extract<ContentBlock, { type: "tool_result" }>;
}

export function ToolResultBlock({ block }: ToolResultProps) {
  const [expanded, setExpanded] = useState(false);
  // 只在内容较长时默认折叠
  const isLong = block.content.length > 200;

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
            "text-xs",
            block.is_error ? "text-error" : "text-text-muted"
          )}
        >
          {block.is_error ? "错误结果" : "工具结果"}
          {!expanded && !isLong && (
            <span className="ml-2 text-text-muted">
              {block.content.slice(0, 80)}
              {block.content.length > 80 ? "..." : ""}
            </span>
          )}
        </span>
      </button>

      {(expanded || !isLong) && (
        <div className="border-t border-border px-3 py-2">
          <pre className="overflow-x-auto whitespace-pre-wrap text-xs text-text-secondary">
            {block.content}
          </pre>
        </div>
      )}
    </div>
  );
}
