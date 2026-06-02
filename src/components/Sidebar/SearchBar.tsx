import { Search, X } from "lucide-react";
import { useSessionStore } from "../../stores/sessionStore";
import { useDebounce } from "../../hooks/useDebounce";
import { useLocale } from "../../i18n";

export function SearchBar() {
  const { searchQuery, setSearchQuery, searchSessions, toolFilter } =
    useSessionStore();
  const { t } = useLocale();

  // 两级防抖搜索：
  // 1. 300ms：廉价的标题/路径实时过滤，保证即时反馈
  useDebounce(
    () => {
      searchSessions(searchQuery, toolFilter);
    },
    300,
    [searchQuery]
  );
  // 2. 1s：停止输入后自动升级为会话内容全文搜索
  //    （与 Enter 等价，用户无需知道快捷键；空查询交由上面的浅搜索回到列表）
  useDebounce(
    () => {
      if (searchQuery.trim()) {
        searchSessions(searchQuery, toolFilter, true);
      }
    },
    1000,
    [searchQuery]
  );

  return (
    <div className="relative px-3 py-2">
      <Search
        size={14}
        className="absolute left-5 top-1/2 -translate-y-1/2 text-text-muted"
      />
      <input
        type="text"
        placeholder={t.searchPlaceholder}
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        onKeyDown={(e) => {
          // Enter 立即触发会话内容全文搜索（不必等 1s 自动升级）
          if (e.key === "Enter" && searchQuery.trim()) {
            searchSessions(searchQuery, toolFilter, true);
          }
        }}
        className="w-full rounded-md border border-border bg-surface py-1.5 pl-8 pr-8 text-sm
                   text-text-primary placeholder:text-text-muted
                   focus:border-accent focus:outline-none"
      />
      {searchQuery && (
        <button
          onClick={() => setSearchQuery("")}
          className="absolute right-5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
