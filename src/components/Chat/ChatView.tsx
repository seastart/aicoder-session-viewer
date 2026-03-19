import { useRef, useEffect } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { TOOL_CONFIG } from "../../types";
import { MessageBubble } from "./MessageBubble";
import { Folder, Clock, MessageSquare } from "lucide-react";
import { format } from "date-fns";
import { zhCN } from "date-fns/locale";

export function ChatView() {
  const { currentSession, loading } = useSessionStore();
  const scrollRef = useRef<HTMLDivElement>(null);

  // 切换 session 时滚动到顶部
  useEffect(() => {
    scrollRef.current?.scrollTo(0, 0);
  }, [currentSession?.summary.id]);

  if (!currentSession) {
    return (
      <div className="flex h-full items-center justify-center text-text-muted">
        <div className="text-center">
          <MessageSquare size={48} className="mx-auto mb-4 opacity-30" />
          <p>选择一个 session 查看对话</p>
        </div>
      </div>
    );
  }

  const { summary, messages } = currentSession;
  const config = TOOL_CONFIG[summary.tool];

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
                locale: zhCN,
              })}
            </span>
          )}
          <span className="flex items-center gap-1">
            <MessageSquare size={12} />
            {messages.length} 条消息
          </span>
        </div>
      </div>

      {/* 消息列表 */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex h-full items-center justify-center text-text-muted">
            加载中...
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
