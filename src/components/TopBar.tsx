import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  ChevronDown,
  Copy,
  Folder,
  LayoutGrid,
  Menu as MenuIcon,
  Minus,
  Search,
  Square,
  X,
} from "lucide-react";
import { useStore } from "../lib/store";
import { isTauri, win } from "../lib/tauri";
import { EASE, D, menuPop } from "../lib/motion";
import { FluxMark } from "./ui";
import MainMenu from "./topbar/MainMenu";
import ProjectSwitcher from "./topbar/ProjectSwitcher";
import SearchSessions from "./topbar/SearchSessions";
import { projectNameOrFallback } from "../lib/projects";

type Panel = "menu" | "search" | "switcher" | null;

/**
 * Custom top navigation bar (replaces the native title bar).
 * Left: main menu, Projects/Sessions home, Search sessions.
 * Next: active project identity (opens the switcher), then a drag surface.
 * Right: minimize / maximize / close.
 */
export default function TopBar() {
  const t = useStore((s) => s.t);
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const activeProject = useStore((s) => s.activeProject());

  const [panel, setPanel] = useState<Panel>(null);
  const [maximized, setMaximized] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Track maximize/restore state for the correct window button glyph.
  useEffect(() => {
    if (!isTauri) return;
    let alive = true;
    let unlisten: (() => void) | null = null;
    const sync = () => {
      win
        .isMaximized()
        .then((m) => {
          if (alive) setMaximized(Boolean(m));
        })
        .catch(() => {});
    };
    sync();
    win
      .onResized(sync)
      .then((fn) => {
        if (alive) unlisten = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  // Click-outside + Escape close any open top-bar panel.
  useEffect(() => {
    if (!panel) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setPanel(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPanel(null);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [panel]);

  const togglePanel = (p: Exclude<Panel, null>) => setPanel((cur) => (cur === p ? null : p));

  const topBtn =
    "flex h-7 w-7 items-center justify-center rounded-md text-dim transition-colors hover:bg-hover hover:text-ink";

  const popTransition = { duration: D.fast + 0.02, ease: EASE };

  return (
    <header
      ref={rootRef}
      data-tauri-drag-region
      className="relative z-[80] flex h-10 flex-none items-center gap-1 border-b border-line bg-bg pl-2 pr-0"
      data-testid="topbar"
    >
      {/* branding + top buttons */}
      <div className="flex items-center gap-0.5" data-tauri-drag-region>
        <span className="mr-1.5 flex items-center" data-tauri-drag-region>
          <FluxMark size={18} />
        </span>

        <div className="relative">
          <button
            className={`${topBtn} ${panel === "menu" ? "bg-hover text-ink" : ""}`}
            title={t("topbar.menu")}
            aria-label={t("topbar.menu")}
            aria-expanded={panel === "menu"}
            aria-haspopup="menu"
            data-testid="topbar-menu"
            onClick={() => togglePanel("menu")}
          >
            <MenuIcon size={15} />
          </button>
          <AnimatePresence>
            {panel === "menu" && (
              <motion.div
                {...menuPop("down")}
                transition={popTransition}
                className="absolute left-0 top-[calc(100%+6px)] z-[90] origin-top"
              >
                <MainMenu onClose={() => setPanel(null)} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        <div className="relative">
          <button
            className={`${topBtn} ${view === "home" ? "bg-hover text-ink" : ""}`}
            title={t("topbar.home")}
            aria-label={t("topbar.home")}
            data-testid="topbar-home"
            onClick={() => setView("home")}
          >
            <LayoutGrid size={15} />
          </button>
        </div>

        <div className="relative">
          <button
            className={`${topBtn} ${panel === "search" ? "bg-hover text-ink" : ""}`}
            title={t("topbar.search")}
            aria-label={t("topbar.search")}
            aria-expanded={panel === "search"}
            aria-haspopup="dialog"
            data-testid="topbar-search"
            onClick={() => togglePanel("search")}
          >
            <Search size={15} />
          </button>
          <AnimatePresence>
            {panel === "search" && (
              <motion.div
                {...menuPop("down")}
                transition={popTransition}
                className="absolute left-0 top-[calc(100%+6px)] z-[90] origin-top"
              >
                <SearchSessions onClose={() => setPanel(null)} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* project identity → switcher */}
      <div className="relative">
        <button
          data-testid="topbar-project"
          onClick={() => togglePanel("switcher")}
          className="ml-2 flex h-7 min-w-0 max-w-[240px] items-center gap-1.5 rounded-md px-2 text-left transition-colors hover:bg-hover"
          title={activeProject?.path}
          aria-expanded={panel === "switcher"}
          aria-haspopup="menu"
        >
          <Folder size={13} className="shrink-0 text-dim" />
          <span className="min-w-0 truncate text-[12.5px] text-ink">
            {activeProject ? projectNameOrFallback(activeProject) : t("switcher.selectProject")}
          </span>
          <ChevronDown size={12} className="shrink-0 text-faint" />
        </button>
        <AnimatePresence>
          {panel === "switcher" && (
            <motion.div
              {...menuPop("down")}
              transition={popTransition}
              className="absolute left-2 top-[calc(100%+6px)] z-[90] origin-top"
            >
              <ProjectSwitcher onClose={() => setPanel(null)} />
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* drag surface between controls */}
      <div className="min-w-0 flex-1 self-stretch" data-tauri-drag-region />

      {/* window controls */}
      <div className="flex h-10 flex-none items-stretch" data-testid="window-controls">
        <button
          data-testid="win-minimize"
          onClick={() => isTauri && win.minimize().catch(() => {})}
          className="flex w-11 items-center justify-center text-dim transition-colors hover:bg-hover hover:text-ink"
          title="Minimize"
          aria-label="Minimize"
        >
          <Minus size={13} />
        </button>
        <button
          data-testid="win-maximize"
          onClick={() => isTauri && win.toggleMaximize().catch(() => {})}
          className="flex w-11 items-center justify-center text-dim transition-colors hover:bg-hover hover:text-ink"
          title={maximized ? "Restore" : "Maximize"}
          aria-label={maximized ? "Restore" : "Maximize"}
        >
          {maximized ? <Copy size={11} /> : <Square size={11} />}
        </button>
        <button
          data-testid="win-close"
          onClick={() => isTauri && win.close().catch(() => {})}
          className="flex w-11 items-center justify-center text-dim transition-colors hover:bg-[#c4313b] hover:text-white"
          title="Close"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </div>
    </header>
  );
}
