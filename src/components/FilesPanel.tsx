import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronRight, File, FileText, Folder, FolderOpen, RefreshCw } from "lucide-react";
import { useStore } from "../lib/store";
import { api, type TreeNode } from "../lib/tauri";
import { Modal, IconButton, Spinner } from "./ui";

function baseName(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export default function FilesPanel() {
  const t = useStore((s) => s.t);
  const workspace = useStore((s) => s.settings.workspace);
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [viewing, setViewing] = useState<{ path: string; content: string; truncated: boolean } | null>(null);
  const [viewError, setViewError] = useState("");

  const load = useCallback(async () => {
    if (!workspace) {
      setTree([]);
      return;
    }
    setLoading(true);
    try {
      const nodes = await api.listDirTree(workspace, 4);
      setTree(nodes);
    } catch {
      setTree([]);
    } finally {
      setLoading(false);
    }
  }, [workspace]);

  useEffect(() => {
    load().catch(() => {});
    setExpanded(new Set());
  }, [load]);

  const visible = useMemo(() => {
    const out: { node: TreeNode; open: boolean }[] = [];
    const stack: { open: boolean }[] = [];
    for (const node of tree) {
      const d = Math.min(node.depth, stack.length);
      stack.length = d;
      const parentOpen = d === 0 || stack.slice(0, d).every((x) => x.open);
      if (!parentOpen) continue;
      const open = node.kind === "dir" ? expanded.has(node.path) : false;
      if (node.kind === "dir") stack.push({ open });
      out.push({ node, open });
      if (out.length > 1200) break;
    }
    return out;
  }, [tree, expanded]);

  const toggleDir = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const openFile = async (path: string) => {
    setViewError("");
    setViewing({ path, content: "", truncated: false });
    try {
      const res = await api.readTextFile(path, 200_000);
      setViewing({ path, content: res.content, truncated: res.truncated });
    } catch (e) {
      setViewing(null);
      setViewError(String(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-none h-9 items-center gap-1.5 border-b border-line px-2.5">
        <Folder size={13} className="shrink-0 text-faint" />
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-dim" title={workspace}>
          {workspace ? baseName(workspace) : t("files.title")}
        </span>
        <IconButton className="h-6 w-6" onClick={() => load().catch(() => {})} title={t("preview.refresh")}>
          {loading ? <Spinner size={12} /> : <RefreshCw size={12} />}
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-1.5">
        {!workspace ? (
          <p className="px-4 py-8 text-center text-[12px] text-faint">{t("files.empty")}</p>
        ) : loading && visible.length === 0 ? (
          <div className="space-y-2 px-3 py-2" aria-busy="true">
            {Array.from({ length: 10 }, (_, i) => (
              <div
                key={i}
                className="skeleton h-4"
                style={{ width: `${58 + ((i * 17) % 34)}%`, marginLeft: (i % 3) * 14 }}
              />
            ))}
          </div>
        ) : visible.length === 0 && !loading ? (
          <p className="px-4 py-8 text-center text-[12px] text-faint">{t("files.empty")}</p>
        ) : (
          visible.map(({ node, open }) => (
            <button
              key={node.path}
              onClick={() => (node.kind === "dir" ? toggleDir(node.path) : openFile(node.path).catch(() => {}))}
              className="group flex w-full items-center gap-1.5 rounded-md py-[3px] pr-2 text-left transition-colors hover:bg-hover/60"
              style={{ paddingLeft: 8 + node.depth * 14 }}
              title={node.path}
            >
              {node.kind === "dir" ? (
                <>
                  <ChevronRight size={12} className={`shrink-0 text-faint transition-transform ${open ? "rotate-90" : ""}`} />
                  {open ? <FolderOpen size={13} className="shrink-0 text-faint" /> : <Folder size={13} className="shrink-0 text-faint" />}
                  <span className="min-w-0 flex-1 truncate text-[12.5px] text-dim group-hover:text-ink">{node.name}</span>
                </>
              ) : (
                <>
                  <span className="w-3 shrink-0" />
                  {/\.(md|txt|json|toml|ya?ml|rs|ts|tsx|js|jsx|py|css|html|ini|cfg|log)$/i.test(node.name) ? (
                    <FileText size={13} className="shrink-0 text-faint" />
                  ) : (
                    <File size={13} className="shrink-0 text-faint" />
                  )}
                  <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-dim group-hover:text-ink">{node.name}</span>
                </>
              )}
            </button>
          ))
        )}
      </div>

      {viewError && (
        <div className="flex-none border-t border-line px-3 py-2 font-mono text-[10.5px] text-err">{viewError}</div>
      )}

      <Modal open={!!viewing} onClose={() => setViewing(null)} wide>
        {viewing && (
          <>
            <div className="flex h-11 flex-none items-center gap-2 border-b border-line px-4">
              <FileText size={14} className="text-faint" />
              <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{viewing.path}</span>
              {viewing.truncated && <span className="font-mono text-[10px] text-warn">truncated</span>}
            </div>
            <pre className="min-h-0 flex-1 overflow-auto p-4 font-mono text-[11.5px] leading-relaxed text-dim whitespace-pre-wrap">
              {viewing.content || " "}
            </pre>
          </>
        )}
      </Modal>
    </div>
  );
}
