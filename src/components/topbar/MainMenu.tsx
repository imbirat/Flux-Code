import { useRef, useState } from "react";
import { ChevronRight, FilePlus2, FolderOpen, LogOut, Settings as SettingsIcon } from "lucide-react";
import { useStore } from "../../lib/store";
import { pickAndAddProject } from "../../lib/projects";
import { isTauri, win } from "../../lib/tauri";
import { FluxMark } from "../ui";

/**
 * Main application menu (top button 1). Flux Code header, then a File item
 * with a functional submenu: New Session / Open Project / Settings / Exit.
 * Full arrow-key navigation; Escape closes the submenu first, then the menu.
 */
export default function MainMenu({ onClose }: { onClose: () => void }) {
  const t = useStore((s) => s.t);
  const startNewChat = useStore((s) => s.startNewChat);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  const [subOpen, setSubOpen] = useState(false);
  const [fileIdx, setFileIdx] = useState(-1);
  const fileRef = useRef<HTMLButtonElement>(null);

  const fileItems = [
    {
      key: "new",
      label: t("menu.file.newSession"),
      hint: "Ctrl+N",
      icon: <FilePlus2 size={14} />,
      run: () => {
        onClose();
        startNewChat().catch(() => {});
      },
    },
    {
      key: "open",
      label: t("menu.file.openProject"),
      hint: "",
      icon: <FolderOpen size={14} />,
      run: () => {
        onClose();
        pickAndAddProject().catch(() => {});
      },
    },
    {
      key: "settings",
      label: t("menu.file.settings"),
      hint: "Ctrl+,",
      icon: <SettingsIcon size={14} />,
      run: () => {
        onClose();
        setSettingsOpen(true);
      },
    },
    {
      key: "exit",
      label: t("menu.file.exit"),
      hint: "",
      icon: <LogOut size={14} />,
      run: () => {
        onClose();
        if (isTauri) win.close().catch(() => {});
      },
    },
  ];

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!subOpen) {
      if (e.key === "ArrowRight" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        e.stopPropagation();
        setSubOpen(true);
        setFileIdx(0);
      } else if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
      // ArrowUp/Down have a single item — no-op.
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setFileIdx((i) => Math.min(i + 1, fileItems.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setFileIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (fileIdx >= 0) fileItems[fileIdx].run();
    } else if (e.key === "ArrowLeft" || e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setSubOpen(false);
      setFileIdx(-1);
      fileRef.current?.focus();
    }
  };

  return (
    <div
      role="menu"
      aria-label={t("topbar.menu")}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="relative flex w-[248px] flex-col overflow-visible rounded-xl border border-line bg-raised py-1 shadow-[0_12px_32px_rgba(0,0,0,0.45)] outline-none"
      data-testid="main-menu"
    >
      {/* branding header */}
      <div className="flex items-center gap-2 px-3 pb-2 pt-2.5">
        <FluxMark size={17} />
        <span className="text-[12.5px] font-semibold tracking-tight text-ink">Flux Code</span>
      </div>
      <div className="mb-1 border-t border-line" />

      {/* File */}
      <button
        ref={fileRef}
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={subOpen}
        data-testid="menu-file"
        onMouseEnter={() => setSubOpen(true)}
        onClick={() => {
          setSubOpen((o) => !o);
          setFileIdx(subOpen ? -1 : 0);
        }}
        className={`mx-1 flex h-8 items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] transition-colors ${
          subOpen ? "bg-hover text-ink" : "text-dim hover:bg-hover hover:text-ink"
        }`}
      >
        <span className="min-w-0 flex-1">{t("menu.file")}</span>
        <ChevronRight size={13} className="text-faint" />
      </button>

      {subOpen && (
        <div
          role="menu"
          data-testid="menu-file-sub"
          className="absolute left-[calc(100%-2px)] top-7 z-10 w-[216px] rounded-xl border border-line bg-raised py-1 shadow-[0_12px_32px_rgba(0,0,0,0.5)]"
        >
          {fileItems.map((item, i) => (
            <button
              key={item.key}
              role="menuitem"
              data-testid={`menu-file-${item.key}`}
              onMouseEnter={() => setFileIdx(i)}
              onClick={item.run}
              className={`flex h-8 w-[calc(100%-8px)] mx-1 items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] transition-colors ${
                fileIdx === i ? "bg-hover text-ink" : "text-dim hover:bg-hover hover:text-ink"
              }`}
            >
              <span className="flex h-4 w-4 shrink-0 items-center justify-center">{item.icon}</span>
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
              {item.hint && <span className="font-mono text-[10px] text-faint">{item.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
