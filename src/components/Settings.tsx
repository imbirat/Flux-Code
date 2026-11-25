import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Check,
  FolderPlus,
  History,
  Info,
  KeyRound,
  Languages,
  MessageSquare,
  MoreHorizontal,
  Palette,
  Pencil,
  Plus,
  RefreshCw,
  Shield,
  Trash2,
  X,
} from "lucide-react";
import { useStore } from "../lib/store";
import { api } from "../lib/tauri";
import { LANGUAGES } from "../lib/i18n";
import { EASE, fadeRise } from "../lib/motion";
import { statusDotClass, statusTextClass } from "../lib/providerUtils";
import type { ProviderInfo } from "../lib/types";
import AddKeyModal from "./providers/AddKeyModal";
import SelectModelsModal from "./providers/SelectModelsModal";
import CustomProviderModal from "./providers/CustomProviderModal";
import { FluxMark, IconButton, Menu, MenuDivider, MenuItem, Modal, Spinner } from "./ui";

type Section = "appearance" | "language" | "chats" | "providers" | "permissions" | "about";

const SECTIONS: { id: Section; icon: any; key: string }[] = [
  { id: "appearance", icon: Palette, key: "settings.appearance" },
  { id: "language", icon: Languages, key: "settings.language" },
  { id: "chats", icon: History, key: "settings.chats" },
  { id: "providers", icon: MessageSquare, key: "settings.providers" },
  { id: "permissions", icon: Shield, key: "settings.permissions" },
  { id: "about", icon: Info, key: "settings.about" },
];

const ACCENTS: { id: string; color: string; key: string }[] = [
  { id: "flux", color: "#4da3ff", key: "settings.accent.flux" },
  { id: "blue", color: "#2f6fed", key: "settings.accent.blue" },
  { id: "purple", color: "#8b5cf6", key: "settings.accent.purple" },
  { id: "red", color: "#e5484d", key: "settings.accent.red" },
  { id: "white", color: "#ececee", key: "settings.accent.white" },
];

function Switch({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!on)}
      className={`relative h-5 w-9 shrink-0 rounded-full border transition-colors ${
        on ? "border-accent bg-accentdim" : "border-line bg-bg"
      }`}
    >
      <span
        className={`absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full transition-all ${
          on ? "left-[19px] bg-accent" : "left-[3px] bg-faint"
        }`}
      />
    </button>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-10 items-center justify-between gap-4 border-b border-line py-2.5 last:border-0">
      <div className="min-w-0">
        <div className="text-[13px]">{label}</div>
        {hint && <div className="mt-0.5 text-[11.5px] text-faint">{hint}</div>}
      </div>
      <div className="flex flex-none items-center gap-2">{children}</div>
    </div>
  );
}

function Segmented({ options, value, onChange }: { options: { id: string; label: string }[]; value: string; onChange: (v: string) => void }) {
  return (
    <div className="flex h-8 items-center rounded-lg border border-line bg-bg p-0.5">
      {options.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={`h-7 rounded-md px-2.5 text-[12px] transition-colors ${
            value === o.id ? "bg-hover text-ink" : "text-faint hover:text-dim"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function StatusChip({ status, testing }: { status: string; testing: boolean }) {
  const s = testing ? "testing" : status;
  const label = s === "connected" ? "Connected" : s === "failed" ? "Connection Failed" : s === "testing" ? "Testing…" : s === "not_configured" ? "Add Key" : "Not Tested";
  return (
    <span className="flex h-6 flex-none items-center gap-1.5 rounded-md border border-line bg-bg px-2 text-[11px]">
      <span className={`h-1.5 w-1.5 rounded-full ${testing ? "bg-warn dot-pulse" : statusDotClass(s)}`} />
      <span className={statusTextClass(s)}>{label}</span>
      {testing && <Spinner size={10} />}
    </span>
  );
}

function ProviderRow({
  provider,
  testing,
  onTest,
  onAddKey,
  onSelectModels,
  onEdit,
  onRemove,
}: {
  provider: ProviderInfo;
  testing: boolean;
  onTest: () => void;
  onAddKey: () => void;
  onSelectModels: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const status = testing ? "testing" : provider.status;
  const hasModels = (provider.config.enabledModels ?? []).length > 0;
  const noEndpoint = provider.custom && !provider.baseUrl;

  return (
    <motion.div
      layout="position"
      initial={{ opacity: 0, y: -4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.98 }}
      transition={{ duration: 0.16, ease: EASE }}
      className="flex min-h-11 flex-wrap items-center gap-2 rounded-xl border border-line bg-raised px-3 py-2 transition-colors hover:border-line2"
    >
      <span className={`h-1.5 w-1.5 flex-none rounded-full ${testing ? "bg-warn dot-pulse" : statusDotClass(status)}`} />
      <span className="min-w-0 max-w-[45%] truncate text-[13px] font-medium">{provider.label}</span>
      <span className="hidden flex-none font-mono text-[10px] text-faint sm:inline">{provider.format}</span>
      <span className="flex-1" />
      <StatusChip status={provider.status} testing={testing} />
      {status === "not_configured" && (
        <button
          onClick={onAddKey}
          className="flex h-7 flex-none items-center rounded-lg bg-ink px-3 text-[11.5px] font-medium text-bg transition-opacity hover:opacity-85"
        >
          Add Key
        </button>
      )}
      <button
        onClick={onSelectModels}
        disabled={!hasModels && status !== "connected"}
        className="flex h-7 flex-none items-center rounded-lg border border-line px-2.5 text-[11.5px] text-dim transition-colors enabled:hover:bg-hover enabled:hover:text-ink disabled:opacity-40"
      >
        Select Models
      </button>
      {/* Always available: custom providers must remain editable and removable
          even before a key exists. */}
      <Menu
        width={190}
        align="right"
        trigger={
          <IconButton className="h-7 w-7 flex-none" title="Provider options">
            <MoreHorizontal size={14} />
          </IconButton>
        }
      >
        {provider.custom && <MenuItem icon={<Pencil size={13} />} label="Edit provider" onClick={onEdit} />}
        <MenuItem icon={<KeyRound size={13} />} label="Update key" onClick={onAddKey} />
        <MenuItem icon={<RefreshCw size={13} />} label="Test connection" onClick={onTest} />
        {provider.custom && <MenuDivider />}
        {provider.custom && <MenuItem icon={<Trash2 size={13} />} label="Remove provider" onClick={onRemove} />}
      </Menu>
      {noEndpoint && (
        <span className="w-full truncate text-[10.5px] text-faint">
          No base URL yet — edit to add an endpoint
        </span>
      )}
      {provider.status === "failed" && provider.message && (
        <span className="w-full truncate text-[10.5px] text-err" title={provider.message}>
          {provider.message}
        </span>
      )}
    </motion.div>
  );
}

function ProvidersSection({ providers }: { providers: ProviderInfo[] }) {
  const refreshProviders = useStore((s) => s.refreshProviders);
  const builtins = providers.filter((p) => !p.custom);
  const customs = providers.filter((p) => p.custom);

  const [keyModal, setKeyModal] = useState<ProviderInfo | null>(null);
  const [modelsModal, setModelsModal] = useState<ProviderInfo | null>(null);
  const [customModal, setCustomModal] = useState<{ open: boolean; editing: ProviderInfo | null }>({
    open: false,
    editing: null,
  });
  const [confirmRemove, setConfirmRemove] = useState<ProviderInfo | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

  const runTest = async (p: ProviderInfo) => {
    setTestingId(p.id);
    try {
      await api.providerTest(p.id);
    } catch {
      /* error is persisted in status */
    }
    await refreshProviders();
    setTestingId(null);
  };

  const removeCustom = async () => {
    if (!confirmRemove) return;
    setRemoving(true);
    try {
      await api.customProviderDelete(confirmRemove.id);
      await refreshProviders();
    } catch {
      /* ignore */
    }
    setRemoving(false);
    setConfirmRemove(null);
  };

  return (
    <>
      <h2 className="mb-1 text-[14px] font-semibold">Providers</h2>
      <p className="mb-4 text-[11.5px] leading-relaxed text-faint">
        Keys are stored in Windows Credential Manager. Only providers that pass a real connection
        test show as Connected. Enable the models you want in the chat model picker.
      </p>

      <div className="mb-1.5 text-[10.5px] uppercase tracking-wider text-faint">Providers</div>
      <div className="space-y-1.5">
        {builtins.map((p) => (
          <ProviderRow
            key={p.id}
            provider={p}
            testing={testingId === p.id}
            onTest={() => runTest(p).catch(() => {})}
            onAddKey={() => setKeyModal(p)}
            onSelectModels={() => setModelsModal(p)}
            onEdit={() => undefined}
            onRemove={() => undefined}
          />
        ))}
      </div>

      <div className="mb-1.5 mt-5 flex items-center justify-between">
        <div className="text-[10.5px] uppercase tracking-wider text-faint">Custom providers</div>
      </div>
      <div className="space-y-1.5">
        <AnimatePresence initial={false}>
        {customs.map((p) => (
          <ProviderRow
            key={p.id}
            provider={p}
            testing={testingId === p.id}
            onTest={() => runTest(p).catch(() => {})}
            onAddKey={() => setKeyModal(p)}
            onSelectModels={() => setModelsModal(p)}
            onEdit={() => setCustomModal({ open: true, editing: p })}
            onRemove={() => setConfirmRemove(p)}
          />
        ))}
        </AnimatePresence>
        <button
          onClick={() => setCustomModal({ open: true, editing: null })}
          className="flex h-11 w-full items-center gap-2 rounded-xl border border-dashed border-line px-3 text-[12.5px] text-dim transition-colors hover:border-line2 hover:bg-hover/60 hover:text-ink"
        >
          <Plus size={14} />
          Add Custom Provider
        </button>
        {customs.length === 0 && (
          <p className="px-1 text-[10.5px] leading-relaxed text-faint">
            Connect any OpenAI-, Anthropic- or Gemini-compatible endpoint.
          </p>
        )}
      </div>

      <AddKeyModal provider={keyModal} open={!!keyModal} onClose={() => setKeyModal(null)} />
      <SelectModelsModal
        provider={modelsModal}
        open={!!modelsModal}
        onClose={() => setModelsModal(null)}
      />
      <CustomProviderModal
        open={customModal.open}
        editing={customModal.editing}
        providers={providers}
        onClose={() => setCustomModal({ open: false, editing: null })}
      />
      <Modal open={!!confirmRemove} onClose={() => (removing ? undefined : setConfirmRemove(null))}>
        <div className="px-6 pb-5 pt-6">
          <h2 className="text-[14px] font-semibold tracking-tight">Remove {confirmRemove?.label}?</h2>
          <p className="mt-2 text-[12.5px] leading-relaxed text-dim">
            This removes the provider, its configuration and its stored API key. Your chats and
            project files are kept — chats using this provider will ask you to pick another model.
          </p>
        </div>
        <div className="flex h-14 items-center justify-end gap-2 border-t border-line px-5">
          <button
            onClick={() => (removing ? undefined : setConfirmRemove(null))}
            className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            Cancel
          </button>
          <button
            onClick={() => removeCustom().catch(() => {})}
            disabled={removing}
            className="flex h-8 items-center gap-2 rounded-lg border border-err/50 bg-err/10 px-4 text-[12.5px] font-medium text-err transition-colors enabled:hover:bg-err/20 disabled:opacity-50"
          >
            {removing && <Spinner size={12} />}
            Remove
          </button>
        </div>
      </Modal>
    </>
  );
}

export default function Settings() {
  const t = useStore((s) => s.t);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const settings = useStore((s) => s.settings);
  const applySettings = useStore((s) => s.applySettings);
  const providers = useStore((s) => s.providers);
  const chats = useStore((s) => s.chats);
  const deleteChat = useStore((s) => s.deleteChat);
  const refreshChats = useStore((s) => s.refreshChats);
  const addExtraFolder = useStore((s) => s.addExtraFolder);
  const setWorkspace = useStore((s) => s.setWorkspace);
  const [section, setSection] = useState<Section>("appearance");
  const [version, setVersion] = useState("");
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("0.1.0"));
  }, []);

  return (
    <Modal open={settingsOpen} onClose={() => setSettingsOpen(false)} wide>
      <div className="flex h-[72vh] min-h-0">
        <nav className="flex w-44 flex-none flex-col border-r border-line bg-panel p-2">
          <div className="flex h-9 items-center gap-2 px-2">
            <FluxMark size={18} />
            <span className="text-[13px] font-semibold tracking-tight">{t("settings.title")}</span>
          </div>
          <div className="mt-1 space-y-0.5">
            {SECTIONS.map(({ id, icon: Icon, key }) => (
              <button
                key={id}
                onClick={() => setSection(id)}
                className={`flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-[12.5px] transition-colors ${
                  section === id ? "bg-hover text-ink" : "text-dim hover:bg-hover/60 hover:text-ink"
                }`}
              >
                <Icon size={14} className="shrink-0" />
                {t(key)}
              </button>
            ))}
          </div>
        </nav>

        <div className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-xl px-5 py-5">
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={section}
                initial={fadeRise.initial}
                animate={fadeRise.animate}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.13, ease: EASE }}
              >
            {section === "appearance" && (
              <>
                <h2 className="mb-3 text-[14px] font-semibold">{t("settings.appearance")}</h2>
                <Row label={t("settings.theme")}>
                  <Segmented
                    options={[
                      { id: "system", label: t("settings.theme.system") },
                      { id: "dark", label: t("settings.theme.dark") },
                      { id: "light", label: t("settings.theme.light") },
                    ]}
                    value={settings.theme}
                    onChange={(v) => applySettings({ theme: v as any }).catch(() => {})}
                  />
                </Row>
                <Row label={t("settings.accent")}>
                  <div className="flex items-center gap-1.5">
                    {ACCENTS.map((a) => (
                      <button
                        key={a.id}
                        title={t(a.key)}
                        onClick={() => applySettings({ accent: a.id as any }).catch(() => {})}
                        className={`relative flex h-6 w-6 items-center justify-center rounded-full border transition-all ${
                          settings.accent === a.id ? "border-ink scale-105" : "border-transparent hover:border-line2"
                        }`}
                      >
                        <span className="h-3.5 w-3.5 rounded-full" style={{ background: a.color }} />
                        {settings.accent === a.id && <Check size={10} className="absolute text-bg mix-blend-normal" />}
                      </button>
                    ))}
                  </div>
                </Row>
                <Row label={t("settings.reducedMotion")} hint={t("settings.reducedMotion.hint")}>
                  <Switch on={settings.reducedMotion} onChange={(v) => applySettings({ reducedMotion: v }).catch(() => {})} />
                </Row>
              </>
            )}

            {section === "language" && (
              <>
                <h2 className="mb-3 text-[14px] font-semibold">{t("settings.language")}</h2>
                <div className="space-y-0.5">
                  {LANGUAGES.map((l) => (
                    <button
                      key={l.id}
                      onClick={() => applySettings({ language: l.id }).catch(() => {})}
                      className={`flex h-9 w-full items-center gap-2 rounded-lg px-3 text-left text-[13px] transition-colors ${
                        settings.language === l.id ? "bg-hover text-ink" : "text-dim hover:bg-hover/60 hover:text-ink"
                      }`}
                    >
                      <span className="h-3.5 w-3.5 flex-none">
                        {settings.language === l.id && <Check size={13} className="text-accent" />}
                      </span>
                      {l.label}
                    </button>
                  ))}
                </div>
              </>
            )}

            {section === "chats" && (
              <>
                <div className="mb-3 flex items-center justify-between">
                  <h2 className="text-[14px] font-semibold">{t("settings.chats")}</h2>
                  <button
                    onClick={() => {
                      if (!confirmDeleteAll) {
                        setConfirmDeleteAll(true);
                        return;
                      }
                      Promise.allSettled(chats.map((c) => api.deleteChat(c.id)))
                        .then(() => refreshChats())
                        .catch(() => {});
                      setConfirmDeleteAll(false);
                    }}
                    className={`flex h-7 items-center gap-1.5 rounded-lg border px-2.5 text-[12px] transition-colors ${
                      confirmDeleteAll ? "border-err/50 bg-err/10 text-err" : "border-line text-dim hover:bg-hover hover:text-err"
                    }`}
                  >
                    <Trash2 size={12} />
                    {confirmDeleteAll ? t("settings.deleteAll.confirm") : t("settings.deleteAll")}
                  </button>
                </div>
                {chats.length === 0 ? (
                  <p className="py-8 text-center text-[12px] text-faint">{t("chat.empty.hint")}</p>
                ) : (
                  <div className="space-y-0.5">
                    {chats.map((c) => (
                      <div key={c.id} className="group flex h-9 items-center gap-2 rounded-lg px-2.5 transition-colors hover:bg-hover/60">
                        <MessageSquare size={13} className="shrink-0 text-faint" />
                        <span className="min-w-0 flex-1 truncate text-[13px] text-dim group-hover:text-ink">{c.title}</span>
                        <span className="font-mono text-[10px] text-faint">{new Date(c.updatedAt).toLocaleDateString()}</span>
                        <IconButton className="h-6 w-6 hover:text-err" onClick={() => deleteChat(c.id).catch(() => {})}>
                          <X size={12} />
                        </IconButton>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}

            {section === "providers" && (
              <ProvidersSection providers={providers} />
            )}

            {section === "permissions" && (
              <>
                <h2 className="mb-3 text-[14px] font-semibold">{t("settings.permissions")}</h2>
                <Row label={t("settings.defaultMode")}>
                  <Segmented
                    options={[
                      { id: "ask", label: t("composer.mode.ask") },
                      { id: "full", label: t("composer.mode.full") },
                    ]}
                    value={settings.defaultPermissionMode}
                    onChange={(v) => applySettings({ defaultPermissionMode: v }).catch(() => {})}
                  />
                </Row>
                <Row label={t("settings.workspace")}>
                  <button
                    onClick={async () => {
                      const res = await openDialog({ directory: true }).catch(() => null);
                      const dir = Array.isArray(res) ? res[0] : res;
                      if (typeof dir === "string") setWorkspace(dir).catch(() => {});
                    }}
                    className="flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink"
                  >
                    <FolderPlus size={12} />
                    {t("composer.folder.change")}
                  </button>
                </Row>
                <div className="truncate pb-2 font-mono text-[11px] text-faint">{settings.workspace || "—"}</div>
                <div className="mt-3 mb-1 text-[12px] text-faint">{t("settings.extraFolders")}</div>
                <div className="space-y-0.5">
                  {settings.extraFolders.map((f) => (
                    <div key={f} className="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-hover/60">
                      <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-dim">{f}</span>
                      <IconButton
                        className="h-6 w-6"
                        onClick={() => applySettings({ extraFolders: settings.extraFolders.filter((x) => x !== f) }).catch(() => {})}
                      >
                        <X size={12} />
                      </IconButton>
                    </div>
                  ))}
                  <button
                    onClick={async () => {
                      const res = await openDialog({ directory: true }).catch(() => null);
                      const dir = Array.isArray(res) ? res[0] : res;
                      if (typeof dir === "string") addExtraFolder(dir).catch(() => {});
                    }}
                    className="flex h-8 w-full items-center gap-2 rounded-lg px-2 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
                  >
                    <Plus size={13} />
                    {t("settings.addFolder")}
                  </button>
                </div>
              </>
            )}

            {section === "about" && (
              <div className="flex flex-col items-center pt-10 text-center">
                <FluxMark size={44} />
                <h2 className="mt-4 text-[16px] font-semibold tracking-tight">Flux Code</h2>
                <p className="mt-0.5 font-mono text-[11px] text-faint">
                  {t("settings.version")} {version}
                </p>
                <p className="mt-5 max-w-sm text-[12.5px] leading-relaxed text-dim">{t("settings.about.text")}</p>
              </div>
            )}
              </motion.div>
            </AnimatePresence>
          </div>
        </div>
      </div>
    </Modal>
  );
}
