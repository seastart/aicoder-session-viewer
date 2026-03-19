import { clsx } from "clsx";
import { formatDistanceToNow } from "date-fns";
import { zhCN } from "date-fns/locale";
import { MessageSquare } from "lucide-react";
import { useSessionStore } from "../../stores/sessionStore";
import { TOOL_CONFIG } from "../../types";

export function SessionList() {
  const { sessions, currentSession, selectSession, loading } =
    useSessionStore();

  if (loading && sessions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-text-muted">
        加载中...
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-center text-text-muted">
        未找到 session
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      {sessions.map((session) => {
        const isActive = currentSession?.summary.id === session.id;
        const config = TOOL_CONFIG[session.tool];
        const timeAgo = session.started_at
          ? formatDistanceToNow(new Date(session.started_at), {
              addSuffix: true,
              locale: zhCN,
            })
          : "";

        return (
          <button
            key={`${session.tool}-${session.id}`}
            onClick={() => selectSession(session.tool, session.id)}
            className={clsx(
              "w-full px-3 py-2.5 text-left transition-colors border-b border-border",
              isActive
                ? "bg-surface-hover"
                : "hover:bg-surface-hover"
            )}
          >
            {/* 工具标签 */}
            <div className="mb-1 flex items-center gap-2">
              <span
                className="rounded px-1.5 py-0.5 text-[10px] font-medium"
                style={{
                  backgroundColor: config.bgColor,
                  color: config.color,
                }}
              >
                {config.label}
              </span>
              {timeAgo && (
                <span className="text-[10px] text-text-muted">{timeAgo}</span>
              )}
            </div>

            {/* 标题 */}
            <div className="truncate text-sm text-text-primary" title={session.title}>
              {session.title}
            </div>

            {/* 项目路径 + 消息数 */}
            <div className="mt-0.5 flex items-center gap-2 text-[11px] text-text-muted">
              {session.project_path && (
                <span className="truncate">
                  {session.project_path.split("/").pop()}
                </span>
              )}
              <span className="ml-auto flex items-center gap-0.5">
                <MessageSquare size={10} />
                {session.message_count}
              </span>
            </div>
          </button>
        );
      })}
    </div>
  );
}
