import { clsx } from "clsx";
import { useSessionStore } from "../../stores/sessionStore";
import type { ToolKind } from "../../types";
import { TOOL_CONFIG } from "../../types";

const TOOLS: (ToolKind | null)[] = [null, "claude_code", "codex", "gemini", "open_code"];

export function ToolFilter() {
  const { toolFilter, setToolFilter } = useSessionStore();

  return (
    <div className="flex gap-1 px-3 py-2">
      {TOOLS.map((tool) => {
        const isActive = toolFilter === tool;
        const config = tool ? TOOL_CONFIG[tool] : null;

        return (
          <button
            key={tool ?? "all"}
            onClick={() => setToolFilter(tool)}
            className={clsx(
              "rounded-md px-2 py-1 text-xs font-medium transition-colors",
              isActive
                ? "bg-accent text-surface"
                : "text-text-secondary hover:bg-surface-hover"
            )}
            style={
              isActive && config
                ? { backgroundColor: config.color, color: "#1e1e2e" }
                : undefined
            }
          >
            {config?.label ?? "全部"}
          </button>
        );
      })}
    </div>
  );
}
