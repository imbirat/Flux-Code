import { useCallback, useEffect, useRef, useState } from "react";
import { motion } from "motion/react";
import { Folder, Globe, ListTodo, Monitor, PanelRightClose, PanelRightOpen } from "lucide-react";
import { useStore } from "../lib/store";
import { EASE } from "../lib/motion";
import { IconButton, Tip } from "./ui";
import TodoPanel from "./TodoPanel";
import PreviewPanel from "./PreviewPanel";
import BrowserPanel from "./BrowserPanel";
import FilesPanel from "./FilesPanel";

const TABS = [
  { id: "preview", icon: Monitor, key: "preview.title" },
  { id: "browser", icon: Globe, key: "browser.title" },
  { id: "todo", icon: ListTodo, key: "todo.title" },
  { id: "files", icon: Folder, key: "files.title" },
] as const;

export default function RightPanel() {
  const t = useStore((s) => s.t);
  const rightOpen = useStore((s) => s.rightOpen);
  const rightTab = useStore((s) => s.rightTab);
  const setRightTab = useStore((s) => s.setRightTab);
  const setRightOpen = useStore((s) => s.setRightOpen);
  const servers = useStore((s) => s.servers);
  const todos = useStore((s) => (s.activeChatId ? s.todos[s.activeChatId] : undefined)) ?? [];
  const [width, setWidth] = useState(380);
  const dragging = useRef(false);

  const onDown = useCallback(() => {
    dragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const w = window.innerWidth - e.clientX;
      setWidth(Math.min(720, Math.max(280, w)));
    };
    const onUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const badgeFor = (id: string) =>
    id === "todo" && todos.length > 0 ? todos.length : id === "preview" && servers.length > 0 ? servers.length : 0;

  const railButton = ({ id, icon: Icon, key }: (typeof TABS)[number], showBadge: boolean) => {
    const active = id === rightTab && rightOpen;
    const badge = showBadge ? badgeFor(id) : 0;
    return (
      <Tip key={id} text={t(key)}>
        <motion.button
          whileTap={{ scale: 0.9 }}
          onClick={() => setRightTab(id)}
          className={`relative flex h-8 w-8 items-center justify-center rounded-lg transition-colors ${
            active ? "bg-hover text-ink" : "text-faint hover:bg-hover/60 hover:text-dim"
          }`}
        >
          <Icon size={15} />
          {badge > 0 && (
            <span className="absolute -right-0.5 -top-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-accent px-1 font-mono text-[9px] leading-none text-bg">
              {badge}
            </span>
          )}
        </motion.button>
      </Tip>
    );
  };

  return (
    <motion.aside
      initial={false}
      animate={{ width: rightOpen ? width : 44 }}
      transition={dragging.current ? { duration: 0 } : { duration: 0.2, ease: EASE }}
      className="relative flex flex-none flex-col overflow-hidden border-l border-line bg-panel"
    >
      {/* expanded content */}
      <div
        className={`absolute inset-0 flex flex-col transition-opacity duration-150 ${
          rightOpen ? "opacity-100" : "pointer-events-none opacity-0"
        }`}
        aria-hidden={!rightOpen}
      >
        <div
          onMouseDown={onDown}
          className="absolute inset-y-0 left-0 z-10 w-1 cursor-col-resize transition-colors hover:bg-line2"
        />
        <div className="flex h-11 flex-none items-center gap-0.5 px-2">
          {TABS.map((tab) => {
            const active = tab.id === rightTab;
            return (
              <Tip key={tab.id} text={t(tab.key)}>
                <motion.button
                  whileTap={{ scale: 0.9 }}
                  onClick={() => setRightTab(tab.id)}
                  className={`relative flex h-7 w-7 items-center justify-center rounded-lg transition-colors ${
                    active ? "bg-hover text-ink" : "text-faint hover:bg-hover/60 hover:text-dim"
                  }`}
                >
                  <tab.icon size={14} />
                  {active && (
                    <motion.span
                      layoutId="right-tab-active"
                      className="absolute inset-x-1.5 -bottom-1 h-0.5 rounded-full bg-line2"
                      transition={{ duration: 0.2, ease: EASE }}
                    />
                  )}
                </motion.button>
              </Tip>
            );
          })}
          <span className="min-w-0 flex-1 truncate px-1 text-[11.5px] text-faint">
            {t(TABS.find((x) => x.id === rightTab)?.key ?? "")}
          </span>
          <IconButton onClick={() => setRightOpen(false)} title={t("right.collapse")}>
            <PanelRightClose size={15} />
          </IconButton>
        </div>

        <div className="relative min-h-0 flex-1">
          {TABS.map(({ id }) => (
            <div
              key={id}
              className={`absolute inset-0 transition-opacity duration-150 ${
                rightTab === id ? "pointer-events-auto opacity-100" : "pointer-events-none opacity-0"
              }`}
              aria-hidden={rightTab !== id}
            >
              {id === "preview" && <PreviewPanel />}
              {id === "browser" && <BrowserPanel />}
              {id === "todo" && <TodoPanel />}
              {id === "files" && <FilesPanel />}
            </div>
          ))}
        </div>
      </div>

      {/* collapsed rail */}
      <div
        className={`absolute inset-y-0 left-0 flex w-11 flex-col items-center py-3 transition-opacity duration-150 ${
          rightOpen ? "pointer-events-none opacity-0" : "opacity-100"
        }`}
        aria-hidden={rightOpen}
      >
        <Tip text={t("right.expand")}>
          <IconButton onClick={() => setRightOpen(true)} title={t("right.expand")}>
            <PanelRightOpen size={15} />
          </IconButton>
        </Tip>
        <div className="mt-3 flex flex-col items-center gap-1">{TABS.map((tab) => railButton(tab, true))}</div>
      </div>
    </motion.aside>
  );
}
