import { useEffect, useMemo, useState } from "react";
import { ExternalLink, Monitor, RefreshCw } from "lucide-react";
import { useStore } from "../lib/store";
import { api } from "../lib/tauri";
import { IconButton, Tip } from "./ui";

export default function PreviewPanel() {
  const t = useStore((s) => s.t);
  const servers = useStore((s) => s.servers);
  const processes = useStore((s) => s.processes);
  const [selected, setSelected] = useState(0);
  const [nonce, setNonce] = useState(0);

  const newest = useMemo(() => servers.length - 1, [servers]);
  useEffect(() => {
    if (selected > newest) setSelected(Math.max(0, newest));
  }, [newest, selected]);

  const alive = (processId: number) =>
    processes.some((p) => p.id === processId && p.exited === 0);

  const server = servers[selected];
  const url = server?.url ?? "";

  return (
    <div className="flex h-full flex-col">
      {servers.length > 0 ? (
        <>
          <div className="flex flex-none items-center gap-1.5 border-b border-line px-2.5 py-1.5">
            <div className="flex min-w-0 flex-1 gap-1 overflow-x-auto">
              {servers.map((s, i) => (
                <button
                  key={`${s.processId}-${s.url}`}
                  onClick={() => setSelected(i)}
                  className={`flex h-6 max-w-40 shrink-0 items-center gap-1.5 rounded-md px-2 font-mono text-[10.5px] transition-colors ${
                    i === selected ? "bg-hover text-ink" : "text-faint hover:bg-hover/60 hover:text-dim"
                  }`}
                >
                  <span
                    className={`h-1.5 w-1.5 shrink-0 rounded-full ${alive(s.processId) ? "bg-ok" : "bg-faint"}`}
                    title={alive(s.processId) ? undefined : t("preview.stopped")}
                  />
                  <span className="truncate">{s.url.replace(/^https?:\/\//, "")}</span>
                </button>
              ))}
            </div>
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
          </div>
          <iframe
            key={`${url}-${nonce}`}
            src={url}
            title={t("preview.title")}
            className="min-h-0 flex-1 border-0 bg-white"
            sandbox="allow-scripts allow-forms allow-popups allow-modals allow-same-origin"
          />
        </>
      ) : (
        <div className="flex h-full flex-col items-center justify-center px-6 text-center">
          <Monitor size={22} className="text-faint" />
          <p className="mt-3 max-w-52 text-[12px] text-faint">{t("preview.empty")}</p>
        </div>
      )}
    </div>
  );
}
