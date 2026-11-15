import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  ArrowUp,
  AtSign,
  Brain,
  FileCode2,
  FolderOpen,
  FolderPlus,
  Image as ImageIcon,
  PanelLeft,
  Plus,
  ShieldCheck,
  Square,
  Zap,
} from "lucide-react";
import { useStore } from "../lib/store";
import { api, mimeForPath } from "../lib/tauri";
import type { Attachment } from "../lib/types";
import { IconButton, Menu, MenuDivider, MenuItem, Tip } from "./ui";

const IMAGE_EXT = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"]);

function baseName(p: string): string {
  const norm = p.replace(/[\\/]+$/, "");
  const i = Math.max(norm.lastIndexOf("\\"), norm.lastIndexOf("/"));
  return i >= 0 ? norm.slice(i + 1) : norm;
}

export default function Composer() {
  const t = useStore((s) => s.t);
  const chat = useStore((s) => s.chats.find((c) => c.id === s.activeChatId));
  const providers = useStore((s) => s.providers);
  const caps = useStore((s) => s.caps);
  const ensureCaps = useStore((s) => s.ensureCaps);
  const send = useStore((s) => s.send);
  const stop = useStore((s) => s.stop);
  const selectModel = useStore((s) => s.selectModel);
  const setEffort = useStore((s) => s.setEffort);
  const setPermissionMode = useStore((s) => s.setPermissionMode);
  const setWorkspace = useStore((s) => s.setWorkspace);
  const addExtraFolder = useStore((s) => s.addExtraFolder);
  const streaming = useStore((s) => (chat ? s.streaming[chat.id] : undefined));
  const status = useStore((s) => (chat ? s.status[chat.id] : undefined));
  const leftOpen = useStore((s) => s.leftOpen);
  const setLeftOpen = useStore((s) => s.setLeftOpen);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  const [text, setText] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [dragging, setDragging] = useState(false);
  const [busyFile, setBusyFile] = useState(false);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const dropSubRef = useRef(false);

  const activeModel = chat?.model ?? "";
  const activeProvider = chat?.provider ?? "";
  const providerLabel = providers.find((p) => p.id === activeProvider)?.label ?? activeProvider;
  const chatCaps = caps[`${activeProvider}|${activeModel}`];

  useEffect(() => {
    if (chat?.provider && chat?.model) ensureCaps(chat.provider, chat.model).catch(() => {});
  }, [chat?.provider, chat?.model, ensureCaps, chat]);

  useEffect(() => {
    const ta = taRef.current;
    if (!ta) return;
    ta.style.height = "0px";
    ta.style.height = `${Math.min(ta.scrollHeight, 208)}px`;
  }, [text]);

  const addPaths = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    setBusyFile(true);
    try {
      const next: Attachment[] = [];
      for (const p of paths.slice(0, 12)) {
        const ext = p.split(".").pop()?.toLowerCase() ?? "";
        if (IMAGE_EXT.has(ext)) {
          try {
            const data = await api.readFileBase64(p);
            next.push({ kind: "image", path: p, name: baseName(p), mime: mimeForPath(p), data });
          } catch {
            next.push({ kind: "file", path: p, name: baseName(p) });
          }
        } else {
          next.push({ kind: "file", path: p, name: baseName(p) });
        }
      }
      setAttachments((prev) => [...prev, ...next].slice(0, 16));
    } finally {
      setBusyFile(false);
    }
  }, []);

  useEffect(() => {
    if (dropSubRef.current) return;
    dropSubRef.current = true;
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "over") setDragging(true);
        else if (e.payload.type === "drop") {
          setDragging(false);
          if (e.payload.paths?.length) addPaths(e.payload.paths).catch(() => {});
        } else setDragging(false);
      })
      .then((fn) => (unlisten = fn))
      .catch(() => {});
    return () => {
      try {
        unlisten?.();
      } catch {
        /* ignore */
      }
      dropSubRef.current = false;
    };
  }, [addPaths]);

  const pickFiles = async () => {
    try {
      const res = await openDialog({ multiple: true });
      if (Array.isArray(res)) await addPaths(res);
      else if (typeof res === "string") await addPaths([res]);
    } catch {
      /* ignore */
    }
  };

  const pickImage = async () => {
    try {
      const res = await openDialog({ multiple: true, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"] }] });
      if (Array.isArray(res)) await addPaths(res);
      else if (typeof res === "string") await addPaths([res]);
    } catch {
      /* ignore */
    }
  };

  const canSend = (text.trim().length > 0 || attachments.length > 0) && !!(chat?.provider && chat?.model);
  const isStreaming = !!streaming || !!status;

  const doSend = () => {
    if (!canSend || isStreaming) return;
    const body = text.trim();
    send(body, attachments).catch(() => {});
    setText("");
    setAttachments([]);
    taRef.current?.focus();
  };

  const modelOptions = useMemo(() => {
    const out: { provider: string; label: string; model: string }[] = [];
    for (const p of providers) {
      if (!p.connected) continue;
      const models = (p.config.enabledModels as string[] | undefined) ?? (p.config.defaultModel ? [p.config.defaultModel as string] : []);
      for (const m of models) out.push({ provider: p.id, label: p.label, model: m });
    }
    return out;
  }, [providers]);

  const modelMissing = !!chat && (!chat.provider || !chat.model) && modelOptions.length > 0;

  return (
    <div className="flex-none px-4 pb-4">
      <div className="mx-auto w-full max-w-3xl">
        <div
          className={`rounded-2xl border bg-panel transition-colors ${
            dragging ? "border-accent" : "border-line focus-within:border-line2"
          }`}
        >
          {attachments.length > 0 && (
            <div className="flex flex-wrap gap-1.5 border-b border-line px-3 pb-2 pt-2.5">
              {attachments.map((a, i) => (
                <span
                  key={i}
                  className="group flex h-7 max-w-56 items-center gap-1.5 rounded-lg border border-line bg-raised px-2 text-[11.5px] text-dim"
                >
                  {a.kind === "image" ? <ImageIcon size={12} className="shrink-0 text-faint" /> : <FileCode2 size={12} className="shrink-0 text-faint" />}
                  <span className="truncate">{a.name}</span>
                  <button
                    onClick={() => setAttachments((prev) => prev.filter((_, j) => j !== i))}
                    className="ml-0.5 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-ink"
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}

          <textarea
            ref={taRef}
            rows={1}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                doSend();
              }
            }}
            placeholder={modelMissing ? t("composer.noModel") : t("composer.placeholder")}
            disabled={!chat}
            className="max-h-52 w-full resize-none bg-transparent px-3.5 pb-1 pt-3 text-[13.5px] leading-relaxed outline-none placeholder:text-faint"
          />

          <div className="flex h-10 items-center gap-0.5 px-2 pb-1.5">
            <Tip text={leftOpen ? t("sidebar.collapse") : t("sidebar.expand")}>
              <IconButton onClick={() => setLeftOpen(!leftOpen)} title={leftOpen ? t("sidebar.collapse") : t("sidebar.expand")}>
                <PanelLeft size={15} />
              </IconButton>
            </Tip>

            <Menu
              width={270}
              direction="up"
              trigger={
                <Tip text={t("composer.folder.change")}>
                  <button className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                    <FolderOpen size={14} />
                    <span className="max-w-40 truncate">{chat?.workspace ? baseName(chat.workspace) : t("composer.folder.open")}</span>
                  </button>
                </Tip>
              }
            >
              <MenuItem
                icon={<FolderOpen size={14} />}
                label={t("composer.folder.change")}
                onClick={async () => {
                  const res = await openDialog({ directory: true }).catch(() => null);
                  const dir = Array.isArray(res) ? res[0] : res;
                  if (typeof dir === "string") setWorkspace(dir).catch(() => {});
                }}
              />
              <MenuItem
                icon={<FolderPlus size={14} />}
                label={t("composer.folder.add")}
                onClick={async () => {
                  const res = await openDialog({ directory: true }).catch(() => null);
                  const dir = Array.isArray(res) ? res[0] : res;
                  if (typeof dir === "string") addExtraFolder(dir).catch(() => {});
                }}
              />
            </Menu>

            <Tip text={t("composer.attach.file")}>
              <IconButton onClick={pickFiles} title={t("composer.attach.file")}>
                {busyFile ? <AtSign size={14} className="spin" /> : <Plus size={15} />}
              </IconButton>
            </Tip>

            <div className="flex-1" />

            {chat && (
              <>
                <Menu
                  width={240}
                  direction="up"
                  trigger={
                    <Tip text={t("settings.permissions")}>
                      <button className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                        <ShieldCheck size={14} />
                        <span className="hidden sm:inline">{chat.permissionMode === "full" ? t("composer.mode.full") : t("composer.mode.ask")}</span>
                      </button>
                    </Tip>
                  }
                >
                  <MenuItem label={t("composer.mode.ask")} active={chat.permissionMode !== "full"} onClick={() => setPermissionMode("ask").catch(() => {})} />
                  <MenuItem label={t("composer.mode.full")} active={chat.permissionMode === "full"} onClick={() => setPermissionMode("full").catch(() => {})} />
                </Menu>

                {chatCaps?.supports && chatCaps.values.length > 0 && (
                  <Menu
                    width={200}
                    direction="up"
                    trigger={
                      <Tip text={t("composer.effort")}>
                        <button className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                          <Brain size={14} />
                          <span className="font-mono text-[11px]">{chat.effort || chatCaps.default}</span>
                        </button>
                      </Tip>
                    }
                  >
                  {chatCaps.values.map((v) => (
                    <MenuItem
                      key={v}
                      label={v}
                      active={(chat.effort || chatCaps.default) === v}
                      hint={v === (chat.effort || chatCaps.default) ? "●" : undefined}
                      onClick={() => setEffort(v).catch(() => {})}
                    />
                  ))}
                  </Menu>
                )}

                <Menu
                  width={280}
                  direction="up"
                  align="right"
                  trigger={
                    <Tip text={t("composer.model")}>
                      <button className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                        <Zap size={14} />
                        <span className="max-w-44 truncate font-mono text-[11px]">{activeModel || t("composer.model")}</span>
                      </button>
                    </Tip>
                  }
                >
                  {modelOptions.length === 0 && (
                    <MenuItem icon={<Plus size={14} />} label={t("composer.noModel")} onClick={() => setSettingsOpen(true)} />
                  )}
                  {modelOptions.map((o) => (
                    <MenuItem
                      key={`${o.provider}|${o.model}`}
                      label={
                        <span className="flex items-baseline gap-2">
                          <span className="truncate">{o.model}</span>
                          <span className="text-[10px] text-faint">{o.label}</span>
                        </span>
                      }
                      active={o.provider === activeProvider && o.model === activeModel}
                      onClick={() => selectModel(o.provider, o.model).catch(() => {})}
                    />
                  ))}
                  {modelOptions.length > 0 && (
                    <>
                      <MenuDivider />
                      <MenuItem icon={<Plus size={14} />} label={t("providers.selectModels")} onClick={() => setSettingsOpen(true)} />
                    </>
                  )}
                </Menu>
              </>
            )}

            {isStreaming ? (
              <Tip text={t("composer.stop")}>
                <button
                  onClick={() => stop().catch(() => {})}
                  className="ml-1 flex h-8 w-8 items-center justify-center rounded-xl border border-line text-dim transition-colors hover:bg-hover hover:text-ink"
                  title={t("composer.stop")}
                >
                  <Square size={13} />
                </button>
              </Tip>
            ) : (
              <Tip text={t("composer.send")}>
                <button
                  onClick={doSend}
                  disabled={!canSend}
                  title={t("composer.send")}
                  className="ml-1 flex h-8 w-8 items-center justify-center rounded-xl bg-ink text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-25"
                >
                  <ArrowUp size={15} />
                </button>
              </Tip>
            )}
          </div>
        </div>
        <div className="mt-1.5 text-center font-mono text-[10px] text-faint opacity-60">
          Enter {t("composer.send").toLowerCase()} · Shift+Enter newline
        </div>
      </div>
    </div>
  );
}
