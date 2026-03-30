import { SearchBar } from "./Sidebar/SearchBar";
import { ToolFilter } from "./Sidebar/ToolFilter";
import { SessionList } from "./Sidebar/SessionList";
import { ProjectTree } from "./Sidebar/ProjectTree";
import { ChatView } from "./Chat/ChatView";
import { useLocale } from "../i18n";
import { useSessionStore } from "../stores/sessionStore";
import { List, FolderTree } from "lucide-react";
import { clsx } from "clsx";

export function Layout() {
  const { t } = useLocale();
  const { viewMode, setViewMode } = useSessionStore();

  return (
    <div className="flex h-screen">
      {/* 侧边栏 */}
      <aside className="flex w-80 shrink-0 flex-col border-r border-border bg-sidebar">
        {/* 标题 + 视图切换 */}
        <div className="shrink-0 border-b border-border px-4 py-3 flex items-center justify-between">
          <h1 className="text-sm font-semibold text-text-primary">
            {t.appTitle}
          </h1>

          {/* 视图模式切换 */}
          <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5">
            <button
              onClick={() => setViewMode("flat")}
              className={clsx(
                "rounded p-1 transition-colors",
                viewMode === "flat"
                  ? "bg-surface-hover text-text-primary"
                  : "text-text-muted hover:text-text-primary"
              )}
              title={t.viewFlat}
            >
              <List size={14} />
            </button>
            <button
              onClick={() => setViewMode("grouped")}
              className={clsx(
                "rounded p-1 transition-colors",
                viewMode === "grouped"
                  ? "bg-surface-hover text-text-primary"
                  : "text-text-muted hover:text-text-primary"
              )}
              title={t.viewGrouped}
            >
              <FolderTree size={14} />
            </button>
          </div>
        </div>

        {/* 搜索 */}
        <SearchBar />

        {/* 工具过滤 */}
        <ToolFilter />

        {/* Session 列表 / 项目树 */}
        {viewMode === "flat" ? <SessionList /> : <ProjectTree />}
      </aside>

      {/* 主内容区 */}
      <main className="flex-1 min-w-0">
        <ChatView />
      </main>
    </div>
  );
}
