import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Check, Plus, RefreshCw, X } from "lucide-react";
import { Modal, Spinner } from "../ui";
import { useStore } from "../../lib/store";
import { api } from "../../lib/tauri";
import { EASE } from "../../lib/motion";
import type { ProviderInfo } from "../../lib/types";
import { filterSearch, mergeManualIds, parseManualEntry } from "../../lib/providerUtils";

/**
 * Model management for a provider: real models from the provider API,
 * search, multi-select, manual model IDs, refresh, and persistence.
 * A failed refresh never wipes previously enabled models.
 */
export default function SelectModelsModal({
  provider,
  open,
  onClose,
}: {
  provider: ProviderInfo | null;
  open: boolean;
  onClose: () => void;
}) {
  const refreshProviders = useStore((s) => s.refreshProviders);
  const [enabled, setEnabled] = useState<string[]>([]);
  const [discovered, setDiscovered] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [manual, setManual] = useState("");
  const [manualError, setManualError] = useState("");
  const [saving, setSaving] = useState(false);

  // Seed from the provider's persisted selection, then load real models.
  useEffect(() => {
    if (!open || !provider) return;
    const seed = [...(provider.config.enabledModels ?? [])];
    const def = provider.config.defaultModel;
    if (typeof def === "string" && def && !seed.includes(def)) seed.push(def);
    setEnabled(seed);
    setDiscovered([]);
    setSearch("");
    setManual("");
    setManualError("");
    setLoadError("");
    load(provider.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider?.id, open]);

  if (!open || !provider) return null;

  const load = async (id: string) => {
    setLoading(true);
    setLoadError("");
    try {
      const models = await api.providerListModels(id);
      setDiscovered(models);
      if (models.length === 0) {
        setLoadError("No models returned — add model IDs manually below.");
      }
    } catch (e) {
      setDiscovered([]);
      setLoadError(String(e).replace(/^Error:\s*/, "").slice(0, 220));
    } finally {
      setLoading(false);
    }
  };

  // All candidate ids: discovered ∪ enabled (enabled models are always listed
  // so a temporary discovery failure cannot lose them).
  const all = mergeManualIds(discovered, enabled);
  const visible = filterSearch(all, search);

  const toggle = (m: string) => {
    setEnabled((cur) => (cur.includes(m) ? cur.filter((x) => x !== m) : [...cur, m]));
  };

  const selectAll = () => {
    setEnabled((cur) => mergeManualIds(cur, visible));
  };

  const clearAll = () => {
    setEnabled((cur) => cur.filter((m) => !visible.includes(m)));
  };

  const addManual = () => {
    const ids = parseManualEntry(manual);
    if (ids.length === 0) return;
    for (const id of ids) {
      if (!/^[A-Za-z0-9._\/\-:~@]+$/.test(id)) {
        setManualError(`"${id.slice(0, 40)}" is not a valid model ID`);
        return;
      }
    }
    setManualError("");
    setDiscovered((cur) => mergeManualIds(cur, ids));
    setEnabled((cur) => mergeManualIds(cur, ids));
    setManual("");
  };

  const save = async () => {
    if (saving) return;
    setSaving(true);
    try {
      const prevDefault = provider.config.defaultModel;
      const defaultModel =
        typeof prevDefault === "string" && prevDefault && enabled.includes(prevDefault)
          ? prevDefault
          : (enabled[0] ?? "");
      await api.providerSetConfig(provider.id, {
        enabledModels: enabled.sort((a, b) => a.localeCompare(b)),
        defaultModel: defaultModel || null,
      });
      await refreshProviders();
      onClose();
    } catch (e) {
      setManualError(String(e).replace(/^Error:\s*/, "").slice(0, 200));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal open={open} onClose={() => (saving ? undefined : onClose())}>
      <div className="flex h-12 items-center border-b border-line px-4">
        <h2 className="min-w-0 flex-1 truncate text-[14px] font-semibold tracking-tight">
          {provider.label} — Select Models
        </h2>
        <button
          onClick={() => (saving ? undefined : onClose())}
          className="flex h-7 w-7 items-center justify-center rounded-lg text-dim transition-colors hover:bg-hover hover:text-ink"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col px-4 pb-4 pt-3">
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search models…"
          className="h-9 w-full rounded-lg border border-line bg-bg px-3 text-[12.5px] outline-none focus:border-line2"
        />

        <div className="mb-1 mt-3 flex items-center justify-between">
          <span className="text-[10.5px] uppercase tracking-wider text-faint">Available models</span>
          <div className="flex items-center gap-1">
            <button
              onClick={selectAll}
              className="h-6 rounded-md px-2 text-[11px] text-dim transition-colors hover:bg-hover hover:text-ink"
            >
              Select All
            </button>
            <button
              onClick={clearAll}
              className="h-6 rounded-md px-2 text-[11px] text-dim transition-colors hover:bg-hover hover:text-ink"
            >
              Clear All
            </button>
          </div>
        </div>

        <div className="min-h-40 max-h-64 flex-1 overflow-y-auto rounded-xl border border-line bg-bg p-1">
          {loading && (
            <div className="flex items-center justify-center gap-2 py-10 text-[12px] text-faint">
              <Spinner size={13} /> Loading models…
            </div>
          )}
          {!loading && visible.length === 0 && (
            <div className="px-3 py-8 text-center text-[11.5px] leading-relaxed text-faint">
              {search
                ? "No models match your search."
                : "No models found — use manual entry below."}
            </div>
          )}
          {!loading &&
            visible.map((m) => {
              const on = enabled.includes(m);
              return (
                <button
                  key={m}
                  onClick={() => toggle(m)}
                  className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-hover"
                >
                  <span
                    className={`flex h-4 w-4 flex-none items-center justify-center rounded border transition-colors ${
                      on ? "border-accent bg-accentdim text-accent" : "border-line"
                    }`}
                  >
                    <AnimatePresence initial={false}>
                      {on && (
                        <motion.span
                          key="tick"
                          initial={{ scale: 0.4, opacity: 0 }}
                          animate={{ scale: 1, opacity: 1 }}
                          exit={{ scale: 0.5, opacity: 0 }}
                          transition={{ duration: 0.13, ease: EASE }}
                          className="flex"
                        >
                          <Check size={11} />
                        </motion.span>
                      )}
                    </AnimatePresence>
                  </span>
                  <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-dim">{m}</span>
                </button>
              );
            })}
        </div>

        {loadError && (
          <div className="mt-2 rounded-lg border border-warn/40 bg-warn/10 px-3 py-1.5 text-[11px] leading-relaxed text-warn">
            {loadError}
          </div>
        )}

        <div className="mt-2.5 flex items-center gap-1.5">
          <input
            value={manual}
            onChange={(e) => setManual(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addManual();
              }
            }}
            placeholder="Add model ID manually"
            spellCheck={false}
            className="h-8 min-w-0 flex-1 rounded-lg border border-line bg-bg px-2.5 font-mono text-[11.5px] outline-none focus:border-line2"
          />
          <button
            onClick={addManual}
            disabled={!manual.trim()}
            className="flex h-8 flex-none items-center gap-1.5 rounded-lg border border-line px-2.5 text-[11.5px] text-dim transition-colors enabled:hover:bg-hover enabled:hover:text-ink disabled:opacity-40"
          >
            <Plus size={12} /> Add
          </button>
          <button
            onClick={() => load(provider.id)}
            disabled={loading}
            className="flex h-8 flex-none items-center gap-1.5 rounded-lg border border-line px-2.5 text-[11.5px] text-dim transition-colors enabled:hover:bg-hover enabled:hover:text-ink disabled:opacity-40"
          >
            {loading ? <Spinner size={11} /> : <RefreshCw size={11} />} Refresh Models
          </button>
        </div>
        {manualError && <p className="mt-1.5 text-[11px] text-err">{manualError}</p>}
      </div>

      <div className="flex h-14 items-center border-t border-line px-4">
        <span className="text-[11.5px] text-faint">
          Enabled: {enabled.length} model{enabled.length === 1 ? "" : "s"}
        </span>
        <span className="flex-1" />
        <button
          onClick={() => (saving ? undefined : onClose())}
          className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
        >
          Cancel
        </button>
        <button
          onClick={() => save().catch(() => {})}
          disabled={saving}
          className="ml-2 flex h-8 items-center gap-2 rounded-lg bg-ink px-4 text-[12.5px] font-medium text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-40"
        >
          {saving && <Spinner size={12} />}
          Save
        </button>
      </div>
    </Modal>
  );
}
