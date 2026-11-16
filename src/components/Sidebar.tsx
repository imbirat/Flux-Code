import { useState } from "react";
import { motion } from "motion/react";
import { PanelLeftClose, Plus, Settings, Trash2, Pencil, X } from "lucide-react";
import { useStore } from "../lib/store";
import { EASE, D } from "../lib/motion";
import { FluxMark, IconButton, Tip } from "./ui";

function timeAgo(ts: number): string {
  const s = Math.floor((Date.now() - ts) / 1000);
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

export default function Sidebar() {
  const t = useStore((s) => s.t);
  const chats = useStore((s) => s.chats);
  const activeChatId = useStore((s) => s.activeChatId);
  const leftOpen = useStore((s) => s.leftOpen);
  const setLeftOpen = useStore((s) => s.setLeftOpen);
  const startNewChat = useStore((s) => s.startNewChat);
  const openChat = useStore((s) => s.openChat);
  const renameChat = useStore((s) => s.renameChat);
  const deleteChat = useStore((s) => s.deleteChat);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [menuFor, setMenuFor] = useState<string | null>(null);

  return (
    <motion.aside
      initial={false}
      animate={{ width: leftOpen ? 240 : 48 }}
      transition={{ duration: D.base + 0.04, ease: EASE }}
      className="relative flex flex-none flex-col overflow-hidden border-r border-line bg-panel"
    >
      {/* expanded content */}
      <div
        className={`absolute inset-y-0 left-0 flex w-60 flex-col transition-opacity duration-150 ${
          leftOpen ? "opacity-100" : "pointer-events-none opacity-0"
        }`}
        aria-hidden={!leftOpen}
      >
        <div className="flex h-12 flex-none items-center gap-2 px-3.5">
          <FluxMark size={22} />
          <span className="text-[13.5px] font-semibold tracking-tight">Flux Code</span>
          <div className="flex-1" />
          <IconButton onClick={() => setLeftOpen(false)} title={t("sidebar.collapse")}>
            <PanelLeftClose size={15} />
          </IconButton>
        </div>

        <div className="px-2.5">
          <button
            onClick={() => startNewChat()}
            className="flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            <Plus size={15} />
            {t("app.newChat")}
          </button>
        </div>

        <div className="mt-5 px-3.5 pb-1.5 text-[11.5px] text-faint">{t("app.recent")}</div>
        <nav className="min-h-0 flex-1 space-y-0.5 overflow-y-auto overflow-x-hidden px-2.5 pb-3">
          {chats.map((c) => {
            const active = c.id === activeChatId;
            const isEditing = editingId === c.id;
            return (
              <div
                key={c.id}
                className={`group relative flex h-8 items-center rounded-lg transition-colors ${
                  active ? "bg-hover" : "hover:bg-hover/60"
                }`}
              >
                {isEditing ? (
                  <input
                    autoFocus
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && draft.trim()) {
                        renameChat(c.id, draft.trim());
                        setEditingId(null);
                      }
                      if (e.key === "Escape") setEditingId(null);
                    }}
                    onBlur={() => setEditingId(null)}
                    className="h-6 w-full rounded-md border border-line2 bg-bg px-2 text-[13px] outline-none"
                  />
                ) : (
                  <button
                    onClick={() => openChat(c.id)}
                    className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg px-2.5 text-left"
                  >
                    <span
                      className={`min-w-0 flex-1 truncate text-[13px] ${active ? "text-ink" : "text-dim group-hover:text-ink"}`}
                    >
                      {c.title}
                    </span>
                    <span className="font-mono text-[10px] text-faint opacity-0 transition-opacity group-hover:opacity-0">
                      {timeAgo(c.updatedAt)}
                    </span>
                  </button>
                )}
                {!isEditing && (
                  <div
                    className={`absolute right-1.5 flex items-center gap-0.5 ${
                      menuFor === c.id ? "flex" : "hidden group-hover:flex"
                    }`}
                  >
                    {menuFor === c.id ? (
                      <>
                        <IconButton
                          className="h-6 w-6"
                          title={t("common.rename")}
                          onClick={() => {
                            setDraft(c.title);
                            setEditingId(c.id);
                            setMenuFor(null);
                          }}
                        >
                          <Pencil size={12} />
                        </IconButton>
                        <IconButton
                          className="h-6 w-6 hover:text-err"
                          title={t("common.delete")}
                          onClick={() => {
                            deleteChat(c.id);
                            setMenuFor(null);
                          }}
                        >
                          <Trash2 size={12} />
                        </IconButton>
                        <IconButton className="h-6 w-6" onClick={() => setMenuFor(null)}>
                          <X size={12} />
                        </IconButton>
                      </>
                    ) : (
                      <IconButton
                        className="h-6 w-6"
                        title={t("common.rename")}
                        onClick={() => setMenuFor(menuFor === c.id ? null : c.id)}
                      >
                        <span className="text-[13px] leading-none tracking-widest">···</span>
                      </IconButton>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </nav>

        <div className="flex-none border-t border-line p-2.5">
          <button
            onClick={() => setSettingsOpen(true)}
            className="flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            <Settings size={15} />
            {t("app.settings")}
          </button>
        </div>
      </div>

      {/* collapsed rail */}
      <div
        className={`absolute inset-y-0 left-0 flex w-12 flex-col items-center py-3 transition-opacity duration-150 ${
          leftOpen ? "pointer-events-none opacity-0" : "opacity-100"
        }`}
        aria-hidden={leftOpen}
      >
        <Tip text={t("sidebar.expand")}>
          <button onClick={() => setLeftOpen(true)} className="mb-3" aria-label={t("sidebar.expand")}>
            <FluxMark size={22} />
          </button>
        </Tip>
        <Tip text={t("app.newChat")}>
          <IconButton onClick={() => startNewChat()} title={t("app.newChat")}>
            <Plus size={16} />
          </IconButton>
        </Tip>
        <div className="flex-1" />
        <Tip text={t("app.settings")}>
          <IconButton onClick={() => setSettingsOpen(true)} title={t("app.settings")}>
            <Settings size={16} />
          </IconButton>
        </Tip>
      </div>
    </motion.aside>
  );
}
