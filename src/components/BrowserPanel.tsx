import { useState } from "react";
import { ExternalLink, Globe, RefreshCw } from "lucide-react";
import { useStore } from "../lib/store";
import { api } from "../lib/tauri";
import { IconButton, Tip } from "./ui";

const normalize = (input: string): string => {
  const v = input.trim();
  if (!v) return "";
  if (/^https?:\/\//i.test(v)) return v;
  if (/^[\w.-]+(\.\w+)+(:\d+)?(\/.*)?$/.test(v)) return `https://${v}`;
  return v;
};

export default function BrowserPanel() {
  const t = useStore((s) => s.t);
  const [value, setValue] = useState("");
  const [url, setUrl] = useState("");
  const [nonce, setNonce] = useState(0);

  const go = () => {
    const n = normalize(value);
    if (n) {
      setUrl(n);
      setNonce((x) => x + 1);
    }
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
          onChange={(e) => setValue(e.target.value)}
          placeholder={t("browser.placeholder")}
          spellCheck={false}
          className="h-6 min-w-0 flex-1 rounded-md border border-line bg-bg px-2 font-mono text-[11.5px] outline-none focus:border-line2"
        />
        <Tip text={t("preview.refresh")}>
          <IconButton className="h-6 w-6" onClick={() => setNonce((n) => n + 1)} title={t("preview.refresh")}>
            <RefreshCw size={12} />
          </IconButton>
        </Tip>
        <Tip text={t("preview.openExternal")}>
          <IconButton className="h-6 w-6" onClick={() => url && api.openExternal(url).catch(() => {})} title={t("preview.openExternal")}>
            <ExternalLink size={12} />
          </IconButton>
        </Tip>
      </form>
      {url ? (
        <iframe key={`${url}-${nonce}`} src={url} title={t("browser.title")} className="min-h-0 flex-1 border-0 bg-white" />
      ) : (
        <div className="flex h-full flex-col items-center justify-center px-6 text-center">
          <Globe size={22} className="text-faint" />
          <p className="mt-3 max-w-56 text-[12px] text-faint">{t("browser.placeholder")}</p>
        </div>
      )}
    </div>
  );
}
