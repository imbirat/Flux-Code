import { useEffect, useState } from "react";
import { ExternalLink, Eye, EyeOff } from "lucide-react";
import { Modal, Spinner } from "../ui";
import { useStore } from "../../lib/store";
import { api } from "../../lib/tauri";
import type { ProviderInfo } from "../../lib/types";

/**
 * Add / update an API key for a provider. Saving stores the key in Windows
 * Credential Manager, then runs a real connection test against the provider.
 * The modal stays open on failure so the user can retry.
 */
export default function AddKeyModal({
  provider,
  open,
  onClose,
}: {
  provider: ProviderInfo | null;
  open: boolean;
  onClose: () => void;
}) {
  const refreshProviders = useStore((s) => s.refreshProviders);
  const [key, setKey] = useState("");
  const [reveal, setReveal] = useState(false);
  const [phase, setPhase] = useState<"idle" | "saving" | "testing">("idle");
  const [error, setError] = useState("");
  const [keySaved, setKeySaved] = useState(false);

  // Reset state whenever a different provider opens.
  useEffect(() => {
    setKey("");
    setReveal(false);
    setPhase("idle");
    setError("");
    setKeySaved(false);
  }, [provider?.id, open]);

  if (!open || !provider) return null;

  const busy = phase !== "idle";

  const runTest = async () => {
    setPhase("testing");
    setError("");
    try {
      const res = await api.providerTest(provider.id);
      await refreshProviders();
      if (res.ok) {
        onClose();
        return;
      }
      setError(res.message || "Connection failed");
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, "").slice(0, 240));
    } finally {
      setPhase("idle");
    }
  };

  const save = async () => {
    const trimmed = key.trim();
    if (!trimmed || busy) return;
    setError("");
    try {
      setPhase("saving");
      await api.providerSetKey(provider.id, trimmed);
      setKeySaved(true);
      setKey("");
      await refreshProviders();
      await runTest();
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, "").slice(0, 240));
      setPhase("idle");
    }
  };

  const helpHost = (() => {
    try {
      return new URL(provider.helpUrl).host;
    } catch {
      return provider.helpUrl;
    }
  })();

  return (
    <Modal open={open} onClose={() => (busy ? undefined : onClose())}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          save().catch(() => {});
        }}
      >
        <div className="px-6 pb-5 pt-6">
          <h2 className="text-center text-[15px] font-semibold tracking-tight">Add API Key</h2>
          <p className="mt-1 text-center text-[12.5px] text-dim">{provider.label}</p>

          <label className="mb-1.5 mt-5 block text-[12px] text-faint" htmlFor="addkey-input">
            API Key
          </label>
          <div className="relative">
            <input
              id="addkey-input"
              type={reveal ? "text" : "password"}
              value={key}
              onChange={(e) => setKey(e.target.value)}
              autoFocus
              autoComplete="off"
              spellCheck={false}
              placeholder={`Enter your ${provider.label} API key`}
              className="h-9 w-full rounded-lg border border-line bg-bg px-3 pr-9 font-mono text-[12px] outline-none focus:border-line2"
            />
            <button
              type="button"
              onClick={() => setReveal((r) => !r)}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-faint transition-colors hover:text-dim"
              tabIndex={-1}
              aria-label={reveal ? "Hide key" : "Show key"}
            >
              {reveal ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>

          {provider.helpUrl && (
            <p className="mt-3 text-[11.5px] text-faint">
              Get your API key from:{" "}
              <a
                href={provider.helpUrl}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-accent hover:underline"
              >
                {helpHost}
                <ExternalLink size={11} />
              </a>
            </p>
          )}

          {keySaved && !error && (
            <p className="mt-3 text-[11.5px] text-ok">Key saved in Windows Credential Manager.</p>
          )}
          {error && (
            <div className="mt-3 rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-[11.5px] leading-relaxed text-err">
              {error}
            </div>
          )}
        </div>

        <div className="flex h-14 items-center justify-end gap-2 border-t border-line px-5">
          {keySaved && error && (
            <span className="mr-auto flex items-center gap-1.5 text-[11.5px] text-faint">
              <Spinner size={11} />
              Key is saved — fix the issue and retry.
            </span>
          )}
          <button
            type="button"
            onClick={() => (busy ? undefined : onClose())}
            className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={busy || (!keySaved && !key.trim())}
            className="flex h-8 items-center gap-2 rounded-lg bg-ink px-4 text-[12.5px] font-medium text-bg transition-opacity enabled:hover:opacity-85 disabled:opacity-40"
          >
            {busy && <Spinner size={12} />}
            {phase === "saving" ? "Saving…" : phase === "testing" ? "Testing…" : keySaved ? "Retry" : "Save"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
