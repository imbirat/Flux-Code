import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Check,
  FolderPlus,
  History,
  Info,
  Languages,
  MessageSquare,
  Palette,
  Plus,
  RefreshCw,
  Shield,
  Trash2,
  X,
} from "lucide-react";
import { useStore } from "../lib/store";
import { api } from "../lib/tauri";
import { LANGUAGES } from "../lib/i18n";
import type { ProviderConfig, ProviderInfo } from "../lib/types";
import { FluxMark, IconButton, Modal, Spinner } from "./ui";

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

function ProviderCard({ provider }: { provider: ProviderInfo }) {
  const t = useStore((s) => s.t);
  const applySettings = useStore((s) => s.applySettings);
  const refreshProviders = useStore((s) => s.refreshProviders);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testMsg, setTestMsg] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [enabled, setEnabled] = useState<string[]>((provider.config.enabledModels as string[]) ?? []);
  const [cfg, setCfg] = useState<ProviderConfig>(provider.config);
  const [showModels, setShowModels] = useState(false);

  const fetchModels = async () => {
    setTesting(true);
    setTestMsg("");
    try {
      const list = await api.providerListModels(provider.id);
      setModels(list);
      setShowModels(true);
      setTestMsg(`${list.length} models`);
    } catch (e) {
      setTestMsg(String(e).replace(/^Error:\s*/, "").slice(0, 140));
    } finally {
      setTesting(false);
    }
  };

  const saveKey = async () => {
    if (!key.trim()) return;
    setSaving(true);
    try {
      await api.providerSetKey(provider.id, key.trim());
      setKey("");
      await refreshProviders();
    } catch {
      /* ignore */
    } finally {
      setSaving(false);
    }
  };

  const testConnection = async () => {
    setTesting(true);
    setTestMsg("");
    try {
      const res = await api.providerTest(provider.id);
      setTestMsg(res.message);
    } catch (e) {
      setTestMsg(String(e).replace(/^Error:\s*/, "").slice(0, 140));
    } finally {
      setTesting(false);
    }
  };

  const persistConfig = async (next: ProviderConfig) => {
    setCfg(next);
    try {
      await api.providerSetConfig(provider.id, next);
      await refreshProviders();
    } catch {
      /* ignore */
    }
  };

  const toggleModel = async (m: string) => {
    const next = enabled.includes(m) ? enabled.filter((x) => x !== m) : [...enabled, m];
    setEnabled(next);
    await persistConfig({ ...cfg, enabledModels: next });
  };

  const isCustom = provider.id === "custom";

  return (
    <div className="overflow-hidden rounded-xl border border-line bg-raised">
      <div className="flex h-10 items-center gap-2 border-b border-line px-3.5">
        <span className={`h-1.5 w-1.5 rounded-full ${provider.connected ? "bg-ok" : "bg-faint"}`} />
        <span className="text-[13px] font-medium">{provider.label}</span>
        <span className="font-mono text-[10px] text-faint">{provider.format}</span>
        <span className="flex-1" />
        {provider.connected && (
          <button onClick={testConnection} disabled={testing} className="flex h-6 items-center gap-1.5 rounded-md px-2 text-[11.5px] text-dim transition-colors hover:bg-hover hover:text-ink">
            {testing ? <Spinner size={11} /> : <RefreshCw size={11} />}
            {testing ? t("providers.testing") : t("providers.test")}
          </button>
        )}
      </div>
      <div className="px-3.5 py-3">
        {!provider.connected ? (
          <div className="flex items-center gap-2">
            <input
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") saveKey().catch(() => {});
              }}
              placeholder={t("providers.keyPlaceholder")}
              className="h-8 min-w-0 flex-1 rounded-lg border border-line bg-bg px-2.5 font-mono text-[12px] outline-none focus:border-line2"
            />
            <button onClick={() => saveKey().catch(() => {})} disabled={saving || !key.trim()} className="h-8 flex-none rounded-lg bg-ink px-3 text-[12px] font-medium text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-40">
              {saving ? "…" : t("providers.save")}
            </button>
          </div>
        ) : (
          <>
            {isCustom && (
              <div className="mb-3 space-y-2">
                <div className="flex gap-2">
                  <input
                    value={(cfg.custom?.name as string) ?? ""}
                    onChange={(e) => setCfg({ ...cfg, custom: { ...cfg.custom, name: e.target.value } })}
                    onBlur={() => persistConfig(cfg).catch(() => {})}
                    placeholder={t("providers.customName")}
                    className="h-8 min-w-0 flex-1 rounded-lg border border-line bg-bg px-2.5 text-[12.5px] outline-none focus:border-line2"
                  />
                  <input
                    value={(cfg.custom?.baseUrl as string) ?? ""}
                    onChange={(e) => setCfg({ ...cfg, custom: { ...cfg.custom, baseUrl: e.target.value } })}
                    onBlur={() => persistConfig(cfg).catch(() => {})}
                    placeholder={t("providers.baseUrl")}
                    className="h-8 min-w-0 flex-1 rounded-lg border border-line bg-bg px-2.5 font-mono text-[11.5px] outline-none focus:border-line2"
                  />
                </div>
                <div className="flex gap-2">
                  <Segmented
                    options={[
                      { id: "openai", label: "OpenAI" },
                      { id: "anthropic", label: "Anthropic" },
                      { id: "gemini", label: "Gemini" },
                    ]}
                    value={(cfg.custom?.format as string) ?? provider.format}
                    onChange={(v) => persistConfig({ ...cfg, custom: { ...cfg.custom, format: v } }).catch(() => {})}
                  />
                </div>
                <textarea
                  value={JSON.stringify(cfg.custom?.headers ?? {}, null, 0)}
                  onChange={(e) => {
                    try {
                      const parsed = JSON.parse(e.target.value || "{}");
                      setCfg({ ...cfg, custom: { ...cfg.custom, headers: parsed } });
                    } catch {
                      /* keep typing */
                    }
                  }}
                  onBlur={() => persistConfig(cfg).catch(() => {})}
                  placeholder={t("providers.headers")}
                  rows={2}
                  className="w-full resize-y rounded-lg border border-line bg-bg px-2.5 py-1.5 font-mono text-[11px] outline-none focus:border-line2"
                />
              </div>
            )}
            <div className="flex items-center gap-2">
              <button onClick={fetchModels} disabled={testing} className="flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink">
                {testing ? <Spinner size={11} /> : <RefreshCw size={12} />}
                {t("providers.refresh")}
              </button>
              <span className="min-w-0 flex-1 truncate text-right font-mono text-[10.5px] text-faint">{testMsg}</span>
              <button
                onClick={() => {
                  try {
                    api.providerRemoveKey(provider.id).then(() => refreshProviders()).catch(() => {});
                  } catch {
                    /* ignore */
                  }
                }}
                className="flex h-7 items-center rounded-lg border border-line px-2.5 text-[12px] text-dim transition-colors hover:bg-hover hover:text-err"
              >
                {t("providers.remove")}
              </button>
            </div>
            {(models.length > 0 || enabled.length > 0) && (
              <div className="mt-2.5">
                <div className="mb-1 text-[11px] text-faint">{t("providers.enabledModels")}</div>
                <div className="max-h-40 space-y-0.5 overflow-y-auto rounded-lg border border-line bg-bg p-1.5">
                  {[...new Set([...enabled, ...models])].map((m) => (
                    <button
                      key={m}
                      onClick={() => toggleModel(m).catch(() => {})}
                      className="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left font-mono text-[11px] transition-colors hover:bg-hover"
                    >
                      <span className={`flex h-3.5 w-3.5 flex-none items-center justify-center rounded border ${enabled.includes(m) ? "border-accent bg-accentdim text-accent" : "border-line"}`}>
                        {enabled.includes(m) && <Check size={10} />}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-dim">{m}</span>
                    </button>
                  ))}
                  {models.length === 0 && (
                    <div className="px-1.5 py-1 text-[11px] text-faint">{t("providers.noModels")}</div>
                  )}
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
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
              <>
                <h2 className="mb-1 text-[14px] font-semibold">{t("settings.providers")}</h2>
                <p className="mb-3 text-[11.5px] text-faint">
                  Keys are stored in Windows Credential Manager. Enable the models you want in the model picker.
                </p>
                <div className="space-y-2.5">
                  {providers.map((p) => (
                    <ProviderCard key={p.id} provider={p} />
                  ))}
                </div>
              </>
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
          </div>
        </div>
      </div>
    </Modal>
  );
}
