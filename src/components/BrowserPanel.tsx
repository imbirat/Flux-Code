import { useState } from "react";
import { ExternalLink, Globe, RefreshCw } from "lucide-react";
import { useStore } from "../lib/store";
import { api } from "../lib/tauri";
import { normalizeHttpUrl } from "../lib/safeUrl";
import { IconButton, Tip } from "./ui";

export default function BrowserPanel() {
  const t = useStore((s) => s.t);
  const [value, setValue] = useState("");
  const [url, setUrl] = useState("");
  const [nonce, setNonce] = useState(0);
  const [error, setError] = useState("");

  const go = () => {
    const normalized = normalizeHttpUrl(value);
    if (!normalized) {
      setError(t("browser.invalid"));
      return;
    }
    setError("");
    setValue(normalized);
    setUrl(normalized);
    setNonce((x) => x + 1);
  };

  return (
    <div className="flex h-full flex-col">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          go();
        }}
        className="flex flex-none h-9 items-center gap-1.5 border-b border-line px-2.5"
      >
        <input
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            if (error) setError("");
          }}
          placeholder={t("browser.placeholder")}
          spellCheck={false}
          aria-invalid={!!error}
          className={`h-6 min-w-0 flex-1 rounded-md border bg-bg px-2 font-mono text-[11.5px] outline-none ${
            error ? "border-err/60 focus:border-err" : "border-line focus:border-line2"
          }`}
        />
        <Tip text={t("preview.refresh")}>
          <IconButton
            className="h-6 w-6"
            onClick={() => url && setNonce((n) => n + 1)}
            title={t("preview.refresh")}
          >
            <RefreshCw size={12} />
          </IconButton>
        </Tip>
        <Tip text={t("preview.openExternal")}>
          <IconButton
            className="h-6 w-6"
            onClick={() => url && api.openExternal(url).catch(() => {})}
            title={t("preview.openExternal")}
          >
            <ExternalLink size={12} />
          </IconButton>
        </Tip>
      </form>
      {error && (
        <div className="flex-none border-b border-err/30 bg-err/10 px-3 py-1.5 text-[11px] text-err">
          {error}
        </div>
      )}
      {url ? (
        <iframe
          key={`${url}-${nonce}`}
          src={url}
          title={t("browser.title")}
          className="min-h-0 flex-1 border-0 bg-white"
          sandbox="allow-scripts allow-forms allow-popups allow-modals"
          referrerPolicy="no-referrer"
        />
      ) : (
        <div className="flex h-full flex-col items-center justify-center px-6 text-center">
          <Globe size={22} className="text-faint" />
          <p className="mt-3 max-w-56 text-[12px] text-faint">{t("browser.placeholder")}</p>
        </div>
      )}
    </div>
  );
}
