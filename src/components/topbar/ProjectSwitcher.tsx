import { useEffect, useMemo, useState } from "react";
import { Check, Folder, Plus } from "lucide-react";
import { useStore } from "../../lib/store";
import { pickAndAddProject, projectNameOrFallback } from "../../lib/projects";
import { Search } from "lucide-react";

/**
 * Project switcher dropdown (top-bar identity). Search by name and path,
 * highlight the current project, select with click/Enter, Add Project at the
 * bottom. Full arrow-key navigation.
 */
export default function ProjectSwitcher({ onClose }: { onClose: () => void }) {
  const t = useStore((s) => s.t);
  const projects = useStore((s) => s.projects);
  const activeProjectId = useStore((s) => s.activeProjectId);
  const setActiveProject = useStore((s) => s.setActiveProject);

  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const [busy, setBusy] = useState(false);

  const query = q.toLowerCase().trim();
  const filtered = useMemo(
    () =>
      projects.filter(
        (p) => !query || p.name.toLowerCase().includes(query) || p.path.toLowerCase().includes(query),
      ),
    [projects, query],
  );
  useEffect(() => setIdx(0), [query]);

  const choose = (id: string) => {
    setActiveProject(id);
    onClose();
  };

  const add = async () => {
    setBusy(true);
    await pickAndAddProject().catch(() => {});
    setBusy(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setIdx((i) => Math.min(i + 1, Math.max(filtered.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (filtered[idx]) choose(filtered[idx].id);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
  };

  return (
    <div
      role="menu"
      aria-label={t("switcher.recent")}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="flex w-[320px] flex-col overflow-hidden rounded-xl border border-line bg-raised shadow-[0_12px_32px_rgba(0,0,0,0.45)] outline-none"
      data-testid="project-switcher"
    >
      {/* search input */}
      <div className="flex h-10 flex-none items-center gap-2 border-b border-line px-3">
        <Search size={13} className="shrink-0 text-faint" />
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={t("switcher.placeholder")}
          data-testid="switcher-input"
          className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-ink outline-none placeholder:text-faint"
        />
      </div>

      {/* project list */}
      <div className="max-h-[300px] overflow-y-auto p-1.5">
        {filtered.length === 0 ? (
          <div className="px-2.5 py-6 text-center text-[12.5px] text-faint" data-testid="switcher-empty">
            {t("switcher.noResults")}
          </div>
        ) : (
          filtered.map((p, i) => {
            const active = p.id === activeProjectId;
            return (
              <button
                key={p.id}
                role="menuitem"
                data-testid={`switcher-project-${i}`}
                title={p.path}
                onMouseEnter={() => setIdx(i)}
                onClick={() => choose(p.id)}
                className={`flex w-[calc(100%-8px)] mx-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors ${
                  idx === i ? "bg-hover" : ""
                }`}
              >
                <Folder size={14} className={active ? "shrink-0 text-accent" : "shrink-0 text-dim"} />
                <span className="min-w-0 flex-1">
                  <span className={`block truncate text-[13px] ${active ? "text-ink" : "text-dim"}`}>
                    {projectNameOrFallback(p)}
                  </span>
                  <span className="block truncate font-mono text-[10px] text-faint">{p.path}</span>
                </span>
                {active && <Check size={13} className="shrink-0 text-accent" data-testid="switcher-current" />}
              </button>
            );
          })
        )}
      </div>

      {/* footer: add project */}
      <div className="border-t border-line p-1.5">
        <button
          data-testid="switcher-add"
          onClick={add}
          className="flex h-8 w-[calc(100%-8px)] mx-1 items-center gap-2 rounded-lg px-2.5 text-left text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
        >
          <Plus size={14} className="shrink-0" />
          <span className="min-w-0 flex-1">{t("switcher.addProject")}</span>
          {busy && <span className="font-mono text-[10px] text-faint">…</span>}
        </button>
      </div>
    </div>
  );
}
