import { useMemo, useState, useEffect } from "react";
import { clsx } from "clsx";
import { formatDistanceToNow } from "date-fns";
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  Plus,
  MessageSquare,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionStore } from "../../stores/sessionStore";
import { TOOL_CONFIG } from "../../types";
import type { SessionSummary, ToolKind } from "../../types";
import { useLocale, type Locale } from "../../i18n";
import {
  buildProjectTree,
  type ProjectNode,
} from "../../utils/buildProjectTree";
import type { Locale as DateLocale } from "date-fns/locale";
import { useAltKeyPressed } from "../../hooks/useAltKeyPressed";
import { YoloHint } from "../common/YoloHint";

export function ProjectTree() {
  const { sessions, currentSession, selectSession, loading, expandedPaths, togglePathExpanded } =
    useSessionStore();
  const { t, dateLocale } = useLocale();

  const tree = useMemo(
    () => buildProjectTree(sessions, t.ungrouped),
    [sessions, t.ungrouped]
  );

  if (loading && sessions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-text-muted">
        {t.loading}
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-center text-text-muted">
        {t.noSessions}
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      {tree.map((node) => (
        <ProjectFolder
          key={node.path || "__ungrouped__"}
          node={node}
          depth={0}
          expanded={expandedPaths.has(node.path)}
          expandedPaths={expandedPaths}
          onToggle={togglePathExpanded}
          currentSessionId={currentSession?.summary.id ?? null}
          onSelect={selectSession}
          dateLocale={dateLocale}
          t={t}
        />
      ))}
    </div>
  );
}

/** 文件夹节点组件 */
function ProjectFolder({
  node,
  depth,
  expanded,
  expandedPaths,
  onToggle,
  currentSessionId,
  onSelect,
  dateLocale,
  t,
}: {
  node: ProjectNode;
  depth: number;
  expanded: boolean;
  expandedPaths: Set<string>;
  onToggle: (path: string) => void;
  currentSessionId: string | null;
  onSelect: (tool: ToolKind, sessionId: string) => void;
  dateLocale: DateLocale;
  t: Locale;
}) {
  const hasContent = node.sessions.length > 0 || node.children.length > 0;
  const [toolMenuOpen, setToolMenuOpen] = useState(false);
  const altPressed = useAltKeyPressed();

  // 点击外部关闭菜单
  useEffect(() => {
    if (!toolMenuOpen) return;
    const close = () => setToolMenuOpen(false);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [toolMenuOpen]);

  /**
   * 启动新 session；bypass=true 时以 YOLO 模式启动。
   * OpenCode 暂不支持 bypass，调用方需要先判断 yoloSupported。
   */
  const handleNewSession = async (
    tool: ToolKind,
    opts: { bypass: boolean } = { bypass: false },
  ) => {
    if (!node.path) return;
    setToolMenuOpen(false);
    const effectiveBypass = opts.bypass && tool !== "open_code";
    try {
      await invoke("open_new_session", {
        tool,
        projectPath: node.path,
        bypassPermissions: effectiveBypass,
      });
    } catch (err) {
      console.error("Failed to open new session:", err);
    }
  };

  return (
    <div>
      {/* 文件夹头部 */}
      <button
        onClick={() => onToggle(node.path)}
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left transition-colors hover:bg-surface-hover border-b border-border"
        style={{ paddingLeft: `${depth * 16 + 12}px` }}
      >
        {/* 展开/折叠箭头 */}
        {hasContent ? (
          expanded ? (
            <ChevronDown size={14} className="shrink-0 text-text-muted" />
          ) : (
            <ChevronRight size={14} className="shrink-0 text-text-muted" />
          )
        ) : (
          <span className="w-3.5 shrink-0" />
        )}

        {/* 文件夹图标 */}
        {expanded ? (
          <FolderOpen size={14} className="shrink-0 text-accent" />
        ) : (
          <Folder size={14} className="shrink-0 text-text-muted" />
        )}

        {/* 目录名 */}
        <span className="truncate text-sm font-medium text-text-primary" title={node.path || node.name}>
          {node.name}
        </span>

        {/* session 数量 */}
        <span className="ml-auto shrink-0 text-[10px] text-text-muted">
          {t.sessionCount(node.totalCount)}
        </span>

        {/* 新建会话按钮 + 工具选择下拉 */}
        {node.path && (
          <span className="relative shrink-0">
            <span
              onClick={(e) => {
                e.stopPropagation();
                setToolMenuOpen(!toolMenuOpen);
              }}
              className="rounded p-0.5 hover:bg-surface-active transition-colors inline-flex"
              title={t.newSession}
            >
              <Plus size={12} className="text-text-muted" />
            </span>

            {toolMenuOpen && (
              <div
                className="absolute right-0 top-full mt-1 z-20 w-36 rounded-md border border-border bg-surface shadow-lg"
                onClick={(e) => e.stopPropagation()}
              >
                {(Object.keys(TOOL_CONFIG) as ToolKind[]).map((tool) => {
                  const cfg = TOOL_CONFIG[tool];
                  const yoloSupported = tool !== "open_code";
                  const showYoloBadge = altPressed && yoloSupported;
                  return (
                    <button
                      key={tool}
                      onClick={(e) =>
                        handleNewSession(tool, { bypass: e.altKey && yoloSupported })
                      }
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        if (yoloSupported) {
                          handleNewSession(tool, { bypass: true });
                        }
                      }}
                      className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-hover transition-colors"
                      title={
                        yoloSupported
                          ? `${cfg.label} · ${t.yoloAltHint}`
                          : t.yoloUnsupportedOpenCode
                      }
                    >
                      <span
                        className="inline-block h-2 w-2 rounded-full"
                        style={{ backgroundColor: cfg.color }}
                      />
                      <span className="text-text-primary">{cfg.label}</span>
                      {showYoloBadge && <YoloHint className="ml-auto" />}
                    </button>
                  );
                })}
              </div>
            )}
          </span>
        )}
      </button>

      {/* 展开内容 */}
      {expanded && (
        <>
          {/* 子文件夹 */}
          {node.children.map((child) => (
            <ProjectFolder
              key={child.path}
              node={child}
              depth={depth + 1}
              expanded={expandedPaths.has(child.path)}
              expandedPaths={expandedPaths}
              onToggle={onToggle}
              currentSessionId={currentSessionId}
              onSelect={onSelect}
              dateLocale={dateLocale}
              t={t}
            />
          ))}

          {/* 该路径下的 sessions */}
          {node.sessions.map((session) => (
            <SessionItem
              key={`${session.tool}-${session.id}`}
              session={session}
              depth={depth + 1}
              isActive={currentSessionId === session.id}
              onSelect={onSelect}
              dateLocale={dateLocale}
            />
          ))}
        </>
      )}
    </div>
  );
}

/** 单个 session 条目（复用 SessionList 的样式） */
function SessionItem({
  session,
  depth,
  isActive,
  onSelect,
  dateLocale,
}: {
  session: SessionSummary;
  depth: number;
  isActive: boolean;
  onSelect: (tool: ToolKind, sessionId: string) => void;
  dateLocale: DateLocale;
}) {
  const config = TOOL_CONFIG[session.tool];
  const timeAgo = session.started_at
    ? formatDistanceToNow(new Date(session.started_at), {
        addSuffix: true,
        locale: dateLocale,
      })
    : "";

  return (
    <button
      onClick={() => onSelect(session.tool, session.id)}
      className={clsx(
        "w-full px-3 py-2 text-left transition-colors border-b border-border",
        isActive ? "bg-surface-hover" : "hover:bg-surface-hover"
      )}
      style={{ paddingLeft: `${depth * 16 + 28}px` }}
    >
      {/* 工具标签 + 时间 */}
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

      {/* 消息数 */}
      <div className="mt-0.5 flex items-center gap-2 text-[11px] text-text-muted">
        <span className="ml-auto flex items-center gap-0.5">
          <MessageSquare size={10} />
          {session.message_count}
        </span>
      </div>
    </button>
  );
}
