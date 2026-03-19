import { Search, X } from "lucide-react";
import { useSessionStore } from "../../stores/sessionStore";
import { useDebounce } from "../../hooks/useDebounce";
import { useLocale } from "../../i18n";

export function SearchBar() {
  const { searchQuery, setSearchQuery, searchSessions, toolFilter } =
    useSessionStore();
  const { t } = useLocale();

  // 输入防抖 300ms 后触发搜索
  useDebounce(
    () => {
      searchSessions(searchQuery, toolFilter);
    },
    300,
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
