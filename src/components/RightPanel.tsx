import { useCallback, useEffect, useRef, useState } from "react";
import { Folder, Globe, ListTodo, Monitor, PanelRightClose, PanelRightOpen } from "lucide-react";
import { useStore } from "../lib/store";
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

  if (!rightOpen) {
    return (
      <aside className="flex w-11 flex-none flex-col items-center border-l border-line bg-panel py-3">
        <Tip text={t("right.expand")}>
          <IconButton onClick={() => setRightOpen(true)} title={t("right.expand")}>
            <PanelRightOpen size={15} />
          </IconButton>
        </Tip>
        <div className="mt-3 flex flex-col items-center gap-1">
          {TABS.map(({ id, icon: Icon, key }) => {
            const active = id === rightTab;
            const badge =
              id === "todo" && todos.length > 0 ? todos.length : id === "preview" && servers.length > 0 ? servers.length : 0;
            return (
              <Tip key={id} text={t(key)}>
                <button
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
                </button>
              </Tip>
            );
          })}
        </div>
      </aside>
    );
  }

  return (
    <aside className="relative flex flex-none flex-col border-l border-line bg-panel" style={{ width }}>
      <div
        onMouseDown={onDown}
        className="absolute inset-y-0 left-0 z-10 w-1 cursor-col-resize transition-colors hover:bg-line2"
      />
      <div className="flex h-11 flex-none items-center gap-0.5 px-2">
        {TABS.map(({ id, icon: Icon, key }) => {
          const active = id === rightTab;
          return (
            <Tip key={id} text={t(key)}>
              <button
                onClick={() => setRightTab(id)}
                title={t(key)}
                className={`flex h-7 w-7 items-center justify-center rounded-lg transition-colors ${
                  active ? "bg-hover text-ink" : "text-faint hover:bg-hover/60 hover:text-dim"
                }`}
              >
                <Icon size={14} />
              </button>
            </Tip>
          );
        })}
        <span className="min-w-0 flex-1 truncate px-1 text-[11.5px] text-faint">{t(TABS.find((x) => x.id === rightTab)?.key ?? "")}</span>
        <IconButton onClick={() => setRightOpen(false)} title={t("right.collapse")}>
          <PanelRightClose size={15} />
        </IconButton>
      </div>
      <div className="min-h-0 flex-1">
        {rightTab === "preview" && <PreviewPanel />}
        {rightTab === "browser" && <BrowserPanel />}
        {rightTab === "todo" && <TodoPanel />}
        {rightTab === "files" && <FilesPanel />}
      </div>
    </aside>
  );
}
