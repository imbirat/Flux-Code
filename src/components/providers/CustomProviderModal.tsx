import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { Modal, Spinner } from "../ui";
import { useStore } from "../../lib/store";
import { api } from "../../lib/tauri";
import type { ProviderInfo } from "../../lib/types";
import {
  slugify,
  validateBaseUrl,
  validateCustomName,
} from "../../lib/providerUtils";

/**
 * Recognized OpenAI-compatible endpoints offered as prefills when the user
 * names a provider after one of them. Unknown names get no guessed endpoint.
 */
const PRESETS: { match: string; label: string; baseUrl: string; format: string }[] = [
  { match: "deepseek", label: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", format: "openai" },
  { match: "qwen", label: "Qwen (DashScope)", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", format: "openai" },
  { match: "mistral", label: "Mistral", baseUrl: "https://api.mistral.ai/v1", format: "openai" },
  { match: "together", label: "Together AI", baseUrl: "https://api.together.xyz/v1", format: "openai" },
  { match: "fireworks", label: "Fireworks AI", baseUrl: "https://api.fireworks.ai/inference/v1", format: "openai" },
  { match: "ollama", label: "Ollama (local)", baseUrl: "http://localhost:11434/v1", format: "openai" },
  { match: "lm studio", label: "LM Studio (local)", baseUrl: "http://localhost:1234/v1", format: "openai" },
  { match: "vllm", label: "vLLM (local)", baseUrl: "http://localhost:8000/v1", format: "openai" },
];

const FORMATS = [
  { id: "openai", label: "OpenAI-compatible" },
  { id: "anthropic", label: "Anthropic-compatible" },
  { id: "gemini", label: "Gemini-compatible" },
];

/**
 * Create or edit a custom provider. The initial form is minimal (name + key);
 * endpoint configuration lives in a collapsed Advanced section. Saving stores
 * config + key, then runs a real connection test when a base URL is present.
 */
export default function CustomProviderModal({
  open,
  editing,
  providers,
  onClose,
}: {
  open: boolean;
  editing: ProviderInfo | null;
  providers: ProviderInfo[];
  onClose: () => void;
}) {
  const refreshProviders = useStore((s) => s.refreshProviders);
  const [name, setName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [format, setFormat] = useState("openai");
  const [headersText, setHeadersText] = useState("{}");
  const [modelsPath, setModelsPath] = useState("");
  const [manualModels, setManualModels] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [headersInvalid, setHeadersInvalid] = useState(false);

  const isEdit = !!editing?.id;

  useEffect(() => {
    if (!open) return;
    setName(editing?.label ?? "");
    setApiKey("");
    setBaseUrl((editing?.config.baseUrl as string) ?? "");
    setFormat((editing?.config.format as string) ?? "openai");
    const h = editing?.config.headers;
    setHeadersText(h && Object.keys(h as object).length ? JSON.stringify(h, null, 2) : "{}");
    setModelsPath((editing?.config.modelsPath as string) ?? "");
    const manual = editing?.config.manualModels as string[] | undefined;
    setManualModels((manual ?? []).join("\n"));
    setAdvanced(false);
    setError("");
    setBusy(false);
    setHeadersInvalid(false);
  }, [open, editing]);

  if (!open) return null;

  const customProviders = providers.filter((p) => p.custom);

  const presetMatch = (() => {
    if (isEdit) return null;
    const n = name.trim().toLowerCase();
    if (!n) return null;
    return PRESETS.find((p) => n.includes(p.match)) ?? null;
  })();

  const applyPreset = () => {
    if (!presetMatch) return;
    if (!baseUrl.trim()) setBaseUrl(presetMatch.baseUrl);
    setFormat(presetMatch.format);
    setAdvanced(true);
  };

  const nameError = validateCustomName(
    name,
    customProviders.map((p) => ({ id: p.id, name: p.label })),
    editing?.id,
  );
  const urlError = validateBaseUrl(baseUrl);
  const canSave =
    !!name.trim() && !nameError && !urlError && !headersInvalid && !busy;

  const save = async () => {
    if (!canSave) return;
    setBusy(true);
    setError("");
    try {
      let headers = {};
      try {
        headers = JSON.parse(headersText || "{}");
      } catch {
        /* guarded by headersInvalid */
      }
      const manualIds = manualModels
        .split(/[\n,]/)
        .map((s) => s.trim())
        .filter(Boolean);
      const payload: Record<string, unknown> = {
        id: editing?.id,
        name: name.trim(),
        format,
        baseUrl: baseUrl.trim().replace(/\/+$/, ""),
        headers,
        modelsPath: modelsPath.trim(),
        manualModels: manualIds,
        enabledModels: editing?.config.enabledModels ?? [],
      };
      if (!isEdit) {
        payload.id = `custom-${slugify(name.trim())}`;
      }
      const saved = await api.customProviderSave(payload, apiKey.trim() || undefined);
      // Run a real connection test when the endpoint is configured.
      let tested: { ok: boolean; message: string } | null = null;
      if (saved.baseUrl) {
        tested = await api.providerTest(saved.id).catch(() => null);
      }
      await refreshProviders();
      if (tested && !tested.ok) {
        // The provider is saved; surface the failure on the card. Close the
        // modal — the connection error is visible in the provider list.
        onClose();
        return;
      }
      onClose();
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, "").slice(0, 240));
      setBusy(false);
    }
  };

  return (
    <Modal open={open} onClose={() => (busy ? undefined : onClose())}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          save().catch(() => {});
        }}
      >
        <div className="px-6 pb-5 pt-6">
          <h2 className="text-center text-[15px] font-semibold tracking-tight">
            {isEdit ? "Edit Custom Provider" : "Add Custom Provider"}
          </h2>

          <label className="mb-1.5 mt-5 block text-[12px] text-faint" htmlFor="cp-name">
            Provider Name
          </label>
          <input
            id="cp-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus={!isEdit}
            spellCheck={false}
            maxLength={40}
            placeholder="Enter provider name"
            className="h-9 w-full rounded-lg border border-line bg-bg px-3 text-[13px] outline-none focus:border-line2"
          />
          {nameError && <p className="mt-1.5 text-[11px] text-err">{nameError}</p>}

          <label className="mb-1.5 mt-3.5 block text-[12px] text-faint" htmlFor="cp-key">
            API Key {isEdit && <span className="text-faint/70">(leave blank to keep current)</span>}
          </label>
          <input
            id="cp-key"
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            placeholder="Enter your API key"
            className="h-9 w-full rounded-lg border border-line bg-bg px-3 font-mono text-[12px] outline-none focus:border-line2"
          />

          {presetMatch && !isEdit && (
            <div className="mt-3 flex items-center justify-between rounded-lg border border-line bg-raised px-3 py-2">
              <span className="text-[11.5px] text-dim">
                Looks like <span className="font-medium text-ink">{presetMatch.label}</span> — prefill its endpoint?
              </span>
              <button
                type="button"
                onClick={applyPreset}
                className="h-7 rounded-md border border-line px-2.5 text-[11.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
              >
                Prefill
              </button>
            </div>
          )}

          <button
            type="button"
            onClick={() => setAdvanced((a) => !a)}
            className="mt-4 flex h-8 w-full items-center gap-2 rounded-lg px-2 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            {advanced ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            Advanced Settings
            <span className="text-[11px] text-faint">
              {baseUrl ? "(endpoint configured)" : "(endpoint, format, headers, model IDs)"}
            </span>
          </button>

          {advanced && (
            <div className="mt-2 space-y-3 rounded-xl border border-line bg-bg/40 p-3">
              <div>
                <label className="mb-1 block text-[11.5px] text-faint" htmlFor="cp-url">
                  API Base URL
                </label>
                <input
                  id="cp-url"
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  spellCheck={false}
                  placeholder="https://api.example.com/v1"
                  className={`h-9 w-full rounded-lg border bg-bg px-3 font-mono text-[11.5px] outline-none focus:border-line2 ${
                    urlError ? "border-err/60" : "border-line"
                  }`}
                />
                {urlError && <p className="mt-1 text-[11px] text-err">{urlError}</p>}
              </div>

              <div>
                <div className="mb-1 text-[11.5px] text-faint">API compatibility format</div>
                <div className="flex flex-wrap gap-1.5">
                  {FORMATS.map((f) => (
                    <button
                      key={f.id}
                      type="button"
                      onClick={() => setFormat(f.id)}
                      className={`h-7 rounded-lg border px-2.5 text-[11.5px] transition-colors ${
                        format === f.id
                          ? "border-line2 bg-hover text-ink"
                          : "border-line text-dim hover:bg-hover/60 hover:text-ink"
                      }`}
                    >
                      {f.label}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <label className="mb-1 block text-[11.5px] text-faint" htmlFor="cp-headers">
                  Custom headers (JSON)
                </label>
                <textarea
                  id="cp-headers"
                  value={headersText}
                  onChange={(e) => {
                    setHeadersText(e.target.value);
                    try {
                      const v = JSON.parse(e.target.value || "{}");
                      setHeadersInvalid(!v || typeof v !== "object" || Array.isArray(v));
                    } catch {
                      setHeadersInvalid(true);
                    }
                  }}
                  rows={2}
                  spellCheck={false}
                  className={`w-full resize-y rounded-lg border bg-bg px-2.5 py-1.5 font-mono text-[11px] outline-none focus:border-line2 ${
                    headersInvalid ? "border-err/60" : "border-line"
                  }`}
                />
                {headersInvalid && <p className="mt-1 text-[11px] text-err">Headers must be a JSON object</p>}
              </div>

              <div>
                <label className="mb-1 block text-[11.5px] text-faint" htmlFor="cp-path">
                  Model discovery path (optional)
                </label>
                <input
                  id="cp-path"
                  value={modelsPath}
                  onChange={(e) => setModelsPath(e.target.value)}
                  spellCheck={false}
                  placeholder="/models"
                  className="h-9 w-full rounded-lg border border-line bg-bg px-3 font-mono text-[11.5px] outline-none focus:border-line2"
                />
              </div>

              <div>
                <label className="mb-1 block text-[11.5px] text-faint" htmlFor="cp-manual">
                  Manual model IDs (one per line)
                </label>
                <textarea
                  id="cp-manual"
                  value={manualModels}
                  onChange={(e) => setManualModels(e.target.value)}
                  rows={3}
                  spellCheck={false}
                  placeholder={"my-model-a\nmy-model-b"}
                  className="w-full resize-y rounded-lg border border-line bg-bg px-2.5 py-1.5 font-mono text-[11px] outline-none focus:border-line2"
                />
                <p className="mt-1 text-[10.5px] leading-relaxed text-faint">
                  Use manual IDs when the endpoint has no model listing. They can be selected in the
                  model picker after saving.
                </p>
              </div>
            </div>
          )}

          {error && (
            <div className="mt-3 rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-[11.5px] leading-relaxed text-err">
              {error}
            </div>
          )}
        </div>

        <div className="flex h-14 items-center justify-end gap-2 border-t border-line px-5">
          <button
            type="button"
            onClick={() => (busy ? undefined : onClose())}
            className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!canSave}
            className="flex h-8 items-center gap-2 rounded-lg bg-ink px-4 text-[12.5px] font-medium text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-40"
          >
            {busy && <Spinner size={12} />}
            {busy ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
