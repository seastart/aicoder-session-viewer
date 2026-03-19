import { SearchBar } from "./Sidebar/SearchBar";
import { ToolFilter } from "./Sidebar/ToolFilter";
import { SessionList } from "./Sidebar/SessionList";
import { ChatView } from "./Chat/ChatView";

export function Layout() {
  return (
    <div className="flex h-screen">
      {/* 侧边栏 */}
      <aside className="flex w-80 shrink-0 flex-col border-r border-border bg-sidebar">
        {/* 标题 */}
        <div className="shrink-0 border-b border-border px-4 py-3">
          <h1 className="text-sm font-semibold text-text-primary">
            AICoder Session Viewer
          </h1>
        </div>

        {/* 搜索 */}
        <SearchBar />

        {/* 工具过滤 */}
        <ToolFilter />

        {/* Session 列表 */}
        <SessionList />
      </aside>

      {/* 主内容区 */}
      <main className="flex-1">
        <ChatView />
      </main>
    </div>
  );
}
