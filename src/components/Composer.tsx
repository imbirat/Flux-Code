import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
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
import { listItem } from "../lib/motion";
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

  const setDraft = useStore((s) => s.setDraft);
  const draft = useStore((s) => (chat ? s.drafts[chat.id] : undefined));
  const text = draft?.text ?? "";
  const attachments = draft?.attachments ?? [];
  const [dragging, setDragging] = useState(false);
  const [busyFile, setBusyFile] = useState(false);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const dropSubRef = useRef(false);
  const sendingRef = useRef(false);

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

  const addPaths = useCallback(
    async (paths: string[]) => {
      const chatId = chat?.id;
      if (!chatId || !paths.length) return;
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
        const current = useStore.getState().drafts[chatId]?.attachments ?? [];
        const merged = [...current];
        for (const a of next) if (!merged.some((x) => x.path === a.path)) merged.push(a);
        setDraft(chatId, { attachments: merged.slice(0, 16) });
      } finally {
        setBusyFile(false);
      }
    },
    [chat?.id, setDraft],
  );

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

  // Only models the user explicitly enabled appear in the picker.
  const modelOptions = useMemo(() => {
    const out: { provider: string; label: string; model: string }[] = [];
    for (const p of providers) {
      const models = (p.config.enabledModels as string[] | undefined) ?? [];
      for (const m of models) out.push({ provider: p.id, label: p.label, model: m });
    }
    return out;
  }, [providers]);

  const groupedOptions = useMemo(() => {
    const groups: { label: string; items: typeof modelOptions }[] = [];
    for (const o of modelOptions) {
      let g = groups.find((x) => x.label === o.label);
      if (!g) {
        g = { label: o.label, items: [] };
        groups.push(g);
      }
      g.items.push(o);
    }
    return groups;
  }, [modelOptions]);

  const modelMissing = !!chat && (!chat.provider || !chat.model) && modelOptions.length > 0;
  // The chat's selected model must still be enabled on its provider.
  const modelUnavailable =
    !!chat &&
    providers.length > 0 &&
    !!chat.provider &&
    !!chat.model &&
    !modelOptions.some((o) => o.provider === chat.provider && o.model === chat.model);

  const canSend =
    (text.trim().length > 0 || attachments.length > 0) &&
    !!(chat?.provider && chat?.model) &&
    !modelUnavailable;
  const isStreaming = !!streaming || !!status;

  const doSend = async () => {
    // isStreaming arrives via an async event, so guard the click itself too.
    if (!canSend || isStreaming || sendingRef.current || !chat) return;
    sendingRef.current = true;
    try {
      // The store consumes the draft only when the message was persisted;
      // failures leave the text and attachments exactly where they were.
      await send(text.trim(), attachments);
    } finally {
      sendingRef.current = false;
      taRef.current?.focus();
    }
  };

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
              <AnimatePresence initial={false}>
                {attachments.map((a, i) => (
                  <motion.span
                    key={`${a.path}-${i}`}
                    layout
                    variants={listItem}
                    initial="initial"
                    animate="animate"
                    exit="exit"
                    className="group flex h-7 max-w-56 items-center gap-1.5 rounded-lg border border-line bg-raised px-2 text-[11.5px] text-dim"
                  >
                    {a.kind === "image" ? <ImageIcon size={12} className="shrink-0 text-faint" /> : <FileCode2 size={12} className="shrink-0 text-faint" />}
                    <span className="truncate">{a.name}</span>
                    <button
                      onClick={() => chat && setDraft(chat.id, { attachments: attachments.filter((_, j) => j !== i) })}
                      className="ml-0.5 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-ink"
                    >
                      ×
                    </button>
                  </motion.span>
                ))}
              </AnimatePresence>
            </div>
          )}

          {modelUnavailable && (
            <div className="flex items-center gap-2 border-b border-err/30 bg-err/10 px-3 py-1.5">
              <span className="h-1.5 w-1.5 flex-none rounded-full bg-err" />
              <span className="min-w-0 flex-1 truncate text-[11.5px] text-err">
                {t("composer.model.unavailable")}
              </span>
              <button
                onClick={() => {
                  const btn = document.querySelector<HTMLButtonElement>("[data-model-menu]");
                  btn?.click();
                }}
                className="flex-none text-[11.5px] text-err underline decoration-err/40 underline-offset-2 hover:decoration-err"
              >
                {t("composer.model")}
              </button>
            </div>
          )}

          <textarea
            ref={taRef}
            rows={1}
            value={text}
            onChange={(e) => chat && setDraft(chat.id, { text: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                doSend().catch(() => {});
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
                      <button data-model-menu className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                        <Zap size={14} />
                        <span className="max-w-44 truncate font-mono text-[11px]">{activeModel || t("composer.model")}</span>
                      </button>
                    </Tip>
                  }
                >
                  {modelOptions.length === 0 && (
                    <MenuItem icon={<Plus size={14} />} label={t("composer.noModel")} onClick={() => setSettingsOpen(true)} />
                  )}
                  {groupedOptions.map((g) => (
                    <div key={g.label}>
                      <div className="px-3 pb-0.5 pt-1.5 text-[10px] uppercase tracking-wider text-faint">
                        {g.label}
                      </div>
                      {g.items.map((o) => (
                        <MenuItem
                          key={`${o.provider}|${o.model}`}
                          label={o.model}
                          active={o.provider === activeProvider && o.model === activeModel}
                          onClick={() => selectModel(o.provider, o.model).catch(() => {})}
                        />
                      ))}
                    </div>
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
                <motion.button
                  whileTap={{ scale: 0.92 }}
                  onClick={() => stop().catch(() => {})}
                  className="ml-1 flex h-8 w-8 items-center justify-center rounded-xl border border-line text-dim transition-colors hover:bg-hover hover:text-ink"
                  title={t("composer.stop")}
                >
                  <Square size={13} />
                </motion.button>
              </Tip>
            ) : (
              <Tip text={t("composer.send")}>
                <motion.button
                  whileTap={{ scale: 0.92 }}
                  onClick={() => doSend().catch(() => {})}
                  disabled={!canSend}
                  title={t("composer.send")}
                  className="ml-1 flex h-8 w-8 items-center justify-center rounded-xl bg-ink text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-25"
                >
                  <ArrowUp size={15} />
                </motion.button>
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
