import { useEffect, useMemo, useState } from "react";
import { MessageSquare } from "lucide-react";
import { useStore } from "../../lib/store";
import { byRecent, projectNameOf, relativeTime, searchSessions } from "../../lib/sessions";
import { useDebounced } from "../../lib/useDebounced";
import { Search } from "lucide-react";

/**
 * Session search dropdown (top button 3). Debounced, client-side search over
 * loaded sessions — the same searchSessions logic the home page uses. Shows
 * recent sessions when the query is empty. Arrow keys move, Enter opens,
 * Escape closes.
 */
export default function SearchSessions({ onClose }: { onClose: () => void }) {
  const t = useStore((s) => s.t);
  const chats = useStore((s) => s.chats);
  const projects = useStore((s) => s.projects);
  const openChat = useStore((s) => s.openChat);

  const [q, setQ] = useState("");
  const debounced = useDebounced(q, 120);
  const [idx, setIdx] = useState(0);

  const results = useMemo(
    () => byRecent(searchSessions(chats, debounced, projects)).slice(0, 9),
    [chats, debounced, projects],
  );
  useEffect(() => setIdx(0), [debounced]);

  const open = (id: string) => {
    onClose();
    openChat(id).catch(() => {});
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setIdx((i) => Math.min(i + 1, Math.max(results.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (results[idx]) open(results[idx].id);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
  };

  return (
    <div
      role="dialog"
      aria-label={t("topbar.search")}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="flex w-[340px] flex-col overflow-hidden rounded-xl border border-line bg-raised shadow-[0_12px_32px_rgba(0,0,0,0.45)] outline-none"
      data-testid="search-sessions"
    >
      {/* search input */}
      <div className="flex h-10 flex-none items-center gap-2 border-b border-line px-3">
        <Search size={13} className="shrink-0 text-faint" />
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={t("search.placeholder")}
          data-testid="search-input"
          className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-ink outline-none placeholder:text-faint"
        />
      </div>

      {/* results */}
      <div className="max-h-[320px] overflow-y-auto p-1.5">
        {!debounced.trim() && (
          <div className="px-2 pb-1 pt-1.5 text-[11.5px] text-faint">{t("search.recent")}</div>
        )}
        {results.length === 0 ? (
          <div className="px-2.5 py-6 text-center text-[12.5px] text-faint" data-testid="search-empty">
            {t("search.noResults")}
          </div>
        ) : (
          results.map((c, i) => (
            <button
              key={c.id}
              role="option"
              aria-selected={idx === i}
              data-testid={`search-result-${i}`}
              onMouseEnter={() => setIdx(i)}
              onClick={() => open(c.id)}
              className={`flex w-[calc(100%-8px)] mx-1 items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left transition-colors ${
                idx === i ? "bg-hover" : ""
              }`}
            >
              <MessageSquare size={13} className="shrink-0 text-dim" />
              <span className="min-w-0 flex-1 truncate text-[13px] text-ink">{c.title}</span>
              {c.projectId && projectNameOf(c, projects) && (
                <span className="shrink-0 truncate text-[11px] text-faint">
                  {projectNameOf(c, projects)}
                </span>
              )}
              <span className="shrink-0 font-mono text-[10px] text-faint">
                {relativeTime(c.updatedAt)}
              </span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
