import { ShieldAlert } from "lucide-react";
import { useStore } from "../lib/store";

export default function PermissionCard() {
  const t = useStore((s) => s.t);
  const permission = useStore((s) => s.permission);
  const respond = useStore((s) => s.respondPermission);
  if (!permission) return null;

  const isCommand = permission.detail?.kind === "command";

  return (
    <div className="mx-auto my-2 w-full max-w-3xl px-4">
      <div className="overflow-hidden rounded-xl border border-line2 bg-raised shadow-[0_8px_28px_rgba(0,0,0,0.35)]">
        <div className="flex items-center gap-2 border-b border-line px-4 py-2.5">
          <ShieldAlert size={14} className="text-warn" />
          <span className="text-[13px] font-medium">{t("permission.title")}</span>
          <span className="ml-auto font-mono text-[10.5px] text-faint">{permission.tool}</span>
        </div>
        <div className="px-4 py-3">
          <p className="text-[13px] text-dim">{permission.summary}</p>
          {isCommand && permission.detail.command && (
            <pre className="mt-2 overflow-x-auto rounded-lg border border-line bg-bg px-3 py-2 font-mono text-[12px] text-ink whitespace-pre-wrap">
              {permission.detail.command}
            </pre>
          )}
          {permission.detail.cwd && (
            <div className="mt-2 flex gap-1.5 font-mono text-[10.5px] text-faint">
              <span>{t("permission.workspace")}:</span>
              <span className="truncate">{permission.detail.cwd}</span>
            </div>
          )}
          {!isCommand && permission.detail.path && (
            <div className="mt-2 truncate font-mono text-[11px] text-faint">{permission.detail.path}</div>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-line px-4 py-2.5">
          <button
            onClick={() => respond(false, false)}
            className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            {t("permission.deny")}
          </button>
          <button
            onClick={() => respond(true, true)}
            className="h-8 rounded-lg border border-line px-3.5 text-[12.5px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            {t("permission.always")}
          </button>
          <button
            onClick={() => respond(true, false)}
            className="h-8 rounded-lg bg-ink px-3.5 text-[12.5px] font-medium text-bg transition-opacity hover:opacity-90"
          >
            {t("permission.allowOnce")}
          </button>
        </div>
      </div>
    </div>
  );
}
