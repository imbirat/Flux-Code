import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  FilePlus2,
  FilePen,
  FileX2,
  FolderPlus,
  List,
  ListChecks,
  MessageCircleQuestion,
  Search,
  SquareTerminal,
  Check as CheckIcon,
  X as XIcon,
  Play,
} from "lucide-react";
import type { DiffRow, MessageMeta } from "../lib/types";
import { Markdown } from "./Markdown";
import { Spinner } from "./ui";
import { useStore } from "../lib/store";

const TOOL_ICONS: Record<string, any> = {
  list_files: List,
  read_file: FilePen,
  create_file: FilePlus2,
  edit_file: FilePen,
  delete_file: FileX2,
  create_folder: FolderPlus,
  rename_file: FilePen,
  search_files: Search,
  run_command: SquareTerminal,
  start_process: Play,
  stop_process: SquareTerminal,
  list_processes: List,
  todo_write: ListChecks,
  ask_user: MessageCircleQuestion,
};

const TOOL_TITLES: Record<string, string> = {
  list_files: "Listed files",
  read_file: "Read file",
  create_file: "Created file",
  edit_file: "Edited file",
  delete_file: "Deleted file",
  create_folder: "Created folder",
  rename_file: "Renamed",
  search_files: "Searched",
  run_command: "Command",
  start_process: "Started process",
  stop_process: "Stopped process",
  list_processes: "Processes",
  todo_write: "Updated to-do list",
  ask_user: "Asked the user",
};

function DiffView({ rows }: { rows: DiffRow[] }) {
  const shown = rows.filter((r) => r.type !== "same" || rows.length <= 60);
  return (
    <div className="my-1 overflow-x-auto rounded-lg border border-line bg-bg">
      {shown.map((r, i) => (
        <div
          key={i}
          className={`diff-row ${r.type === "add" ? "diff-add" : r.type === "del" ? "diff-del" : "diff-same"}`}
        >
          <span className="diff-no">{r.oldNo ?? r.newNo ?? ""}</span>
          <span className="whitespace-pre-wrap break-all">{r.type === "add" ? "+ " : r.type === "del" ? "− " : "  "}{r.text}</span>
        </div>
      ))}
      {rows.length > 400 && <div className="px-3 py-1 font-mono text-[11px] text-faint">…</div>}
    </div>
  );
}

function OutputBlock({ text, label }: { text: string; label?: string }) {
  const [open, setOpen] = useState(false);
  const lines = text.split("\n");
  const collapsed = lines.length > 12 && !open;
  const shown = collapsed ? lines.slice(0, 12).join("\n") : text;
  return (
    <div className="mt-1 overflow-hidden rounded-lg border border-line bg-bg">
      <pre className="max-h-72 overflow-auto px-3 py-2 font-mono text-[11.5px] leading-relaxed text-dim whitespace-pre-wrap">
        {shown}
      </pre>
      {lines.length > 12 && (
        <button
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center gap-1.5 border-t border-line px-3 py-1 font-mono text-[10.5px] text-faint transition-colors hover:text-dim"
        >
          {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          {open ? "Hide" : `Show all ${lines.length} lines`}
          {label && <span className="ml-auto">{label}</span>}
        </button>
      )}
    </div>
  );
}

export function ReadOnlyQuestion({ questions, answers }: { questions: any[]; answers?: Record<string, any> }) {
  const t = useStore((s) => s.t);
  return (
    <div className="mt-2 space-y-2">
      {questions.map((q: any, i: number) => (
        <div key={q.id ?? i} className="text-[12.5px]">
          <div className="text-dim">{q.question}</div>
          <div className="mt-0.5 font-mono text-[11.5px] text-ink">
            {(() => {
              const a = answers?.[q.id];
              if (a == null || (Array.isArray(a) && a.length === 0) || a === "") return t("chat.denied") === "Denied" ? "—" : "—";
              return Array.isArray(a) ? a.join(", ") : String(a);
            })()}
          </div>
        </div>
      ))}
    </div>
  );
}

export function ToolCard({
  name,
  args,
  state,
  result,
  meta,
}: {
  name: string;
  args: any;
  state: string;
  result?: string;
  meta?: MessageMeta;
}) {
  const t = useStore((s) => s.t);
  const Icon = TOOL_ICONS[name] ?? List;
  const title = TOOL_TITLES[name] ?? name;
  const target: string =
    meta?.command ??
    args?.command ??
    meta?.path ??
    args?.path ??
    "";
  const shortTarget = target.length > 64 ? target.slice(0, 61) + "…" : target;

  const stateChip =
    state === "running" ? (
      <Spinner />
    ) : state === "done" ? (
      <CheckIcon size={12} className="text-faint" />
    ) : state === "denied" ? (
      <span className="font-mono text-[10px] text-warn">{t("chat.denied")}</span>
    ) : state === "error" ? (
      <XIcon size={12} className="text-err" />
    ) : null;

  return (
    <div className="my-1.5 overflow-hidden rounded-xl border border-line bg-raised">
      <div className="flex h-8 items-center gap-2 border-b border-line px-3">
        <Icon size={13} className="shrink-0 text-faint" />
        <span className="text-[12px] text-dim">{title}</span>
        {shortTarget && (
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-faint">{shortTarget}</span>
        )}
        {!shortTarget && <span className="flex-1" />}
        {stateChip}
      </div>
      <div className="px-3 py-2">
        {(name === "edit_file" || name === "create_file") && meta?.diff && meta.diff.length > 0 && (
          <DiffView rows={meta.diff} />
        )}
        {name === "run_command" && meta?.command && (
          <>
            <div className="font-mono text-[11.5px] text-ink">&gt; {meta.command}</div>
            {meta.output ? <OutputBlock text={meta.output} /> : state === "running" && <OutputBlock text="" />}
            {meta.timedOut && <div className="mt-1 font-mono text-[11px] text-warn">timed out</div>}
            {typeof meta.exitCode === "number" && meta.exitCode !== 0 && (
              <div className="mt-1 font-mono text-[11px] text-err">exit code {meta.exitCode}</div>
            )}
          </>
        )}
        {(name === "read_file" || name === "list_files" || name === "search_files") && result && (
          <OutputBlock text={result} />
        )}
        {name === "ask_user" && meta?.card && (
          <ReadOnlyQuestion questions={meta.card.questions ?? []} answers={meta.card.answers} />
        )}
        {(name === "start_process" || name === "stop_process" || name === "list_processes") && result && !meta?.command && (
          <div className="font-mono text-[11px] leading-relaxed text-faint whitespace-pre-wrap">{result.length > 400 ? result.slice(0, 400) + "…" : result}</div>
        )}
        {state === "error" && result && !meta?.command && (
          <div className="font-mono text-[11px] leading-relaxed text-err whitespace-pre-wrap">{result}</div>
        )}
        {state === "denied" && result && !meta?.command && (
          <div className="text-[11.5px] text-faint">{result}</div>
        )}
      </div>
    </div>
  );
}

export function InlineMarkdown({ text }: { text: string }) {
  return <Markdown>{text}</Markdown>;
}
