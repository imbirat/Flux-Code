import { useLayoutEffect, useRef, useState } from "react";
import { ArrowDown, X } from "lucide-react";
import { useStore } from "../lib/store";
import type { Attachment, ChatMessage } from "../lib/types";
import { Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";
import { FluxMark, Spinner, Tip } from "./ui";

const STATUS_LABEL: Record<string, string> = {
  thinking: "status.thinking",
  reading: "status.reading",
  writing: "status.writing",
  running: "status.running",
  asking: "status.asking",
  working: "status.working",
  "waiting-for-permission": "status.waiting-for-permission",
};

function Attachments({ items }: { items: Attachment[] }) {
  if (!items?.length) return null;
  return (
    <div className="mt-1.5 flex flex-wrap gap-1.5">
      {items.map((a, i) => (
        <span
          key={i}
          className="flex h-6 max-w-52 items-center gap-1.5 rounded-md border border-line bg-raised px-2 font-mono text-[10.5px] text-faint"
        >
          <span className="truncate">{a.name}</span>
          {a.kind === "folder" && <span className="text-faint">/</span>}
        </span>
      ))}
    </div>
  );
}

function UserMessage({ m }: { m: ChatMessage }) {
  const t = useStore((s) => s.t);
  const images = m.meta?.images ?? [];
  return (
    <div className="group">
      <div className="mb-1 flex items-baseline gap-2">
        <span className="text-[11px] font-medium text-faint">{t("chat.you")}</span>
      </div>
      <div className="whitespace-pre-wrap break-words text-[13.5px] leading-relaxed">{m.content}</div>
      <Attachments items={m.meta?.attachments ?? []} />
      {images.length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-2">
          {images.map((img, i) => (
            <img
              key={i}
              src={`data:${img.mime};base64,${img.data}`}
              alt={img.name}
              className="max-h-44 rounded-lg border border-line"
            />
          ))}
        </div>
      )}
    </div>
  );
}

function AssistantMessage({ m }: { m: ChatMessage }) {
  const thinking = m.meta?.thinking as string | undefined;
  return (
    <div>
      {thinking && <ThinkBlock text={thinking} />}
      <Markdown>{m.content}</Markdown>
    </div>
  );
}

function ThinkBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  if (!text.trim()) return null;
  return (
    <div className="mb-2 overflow-hidden rounded-lg border border-line bg-raised/60">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex h-7 w-full items-center gap-1.5 px-3 text-left font-mono text-[10.5px] text-faint transition-colors hover:text-dim"
      >
        reasoning
        <span className="ml-auto">{open ? "hide" : `${text.length} chars`}</span>
      </button>
      {open && (
        <div className="max-h-60 overflow-y-auto border-t border-line px-3 py-2 font-mono text-[11px] leading-relaxed text-dim whitespace-pre-wrap">
          {text}
        </div>
      )}
    </div>
  );
}

function StreamingMessage({ chatId }: { chatId: string }) {
  const buf = useStore((s) => s.streaming[chatId]);
  if (!buf) return null;
  const showThinking = buf.thinking.length > 0;
  return (
    <div>
      {showThinking && (
        <div className="mb-2 font-mono text-[11px] leading-relaxed text-faint">
          {buf.thinking.length > 400 ? buf.thinking.slice(-400) : buf.thinking}
        </div>
      )}
      <div className="md">
        <Markdown>{buf.text}</Markdown>
        <span className="caret-blink -mb-0.5 ml-0.5 inline-block h-[1.05em] w-[3px] translate-y-[3px] rounded-sm bg-ink" />
      </div>
    </div>
  );
}

export default function ChatView() {
  const t = useStore((s) => s.t);
  const activeChatId = useStore((s) => s.activeChatId);
  const messages = useStore((s) => (activeChatId ? s.messages[activeChatId] : undefined)) ?? [];
  const tools = useStore((s) => (activeChatId ? s.tools[activeChatId] : undefined));
  const status = useStore((s) => (activeChatId ? s.status[activeChatId] : undefined));
  const streaming = useStore((s) => (activeChatId ? s.streaming[activeChatId] : undefined));
  const error = useStore((s) => (activeChatId ? s.errors[activeChatId] : undefined));
  const dismissError = useStore((s) => s.dismissError);
  const booted = useStore((s) => s.booted);

  const scrollRef = useRef<HTMLDivElement>(null);
  const [atBottom, setAtBottom] = useState(true);

  const liveToolCount = tools ? Object.values(tools).filter((t) => t.state === "running").length : 0;
  const liveTools = tools
    ? Object.entries(tools)
        .filter(([, t]) => t.state === "running" && t.name)
        .map(([callId, t]) => [callId, t] as const)
    : [];
  const hasContent =
    messages.length > 0 || streaming || liveToolCount > 0 || (status && status !== "thinking");

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && atBottom) el.scrollTop = el.scrollHeight;
  });

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 160);
  };

  const scrollDown = () => {
    const el = scrollRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
      setAtBottom(true);
    }
  };

  return (
    <div className="relative min-h-0 flex-1">
      <div ref={scrollRef} onScroll={onScroll} className="h-full overflow-y-auto overflow-x-hidden">
        {!hasContent ? (
          <div className="flex h-full flex-col items-center justify-center px-6 pb-24">
            <FluxMark size={40} />
            <h1 className="mt-5 text-[17px] font-semibold tracking-tight">
              {t("chat.empty.title")}
            </h1>
            <p className="mt-2 max-w-sm text-center text-[13px] text-dim">{t("chat.empty.hint")}</p>
          </div>
        ) : (
          <div className="mx-auto w-full max-w-3xl space-y-5 px-4 py-6">
            {messages.map((m) =>
              m.role === "user" ? (
                <UserMessage key={m.id} m={m} />
              ) : m.role === "tool" ? (
                <ToolCard
                  key={m.id}
                  name={(m.meta?.toolName as string) ?? ""}
                  args={m.meta}
                  state={(m.meta?.state as string) ?? "done"}
                  result={m.content}
                  meta={m.meta}
                />
              ) : (
                <AssistantMessage key={m.id} m={m} />
              ),
            )}
            {streaming && <StreamingMessage chatId={activeChatId!} />}
            {liveTools.map(([callId, t]) => (
              <ToolCard key={callId} name={t.name ?? ""} args={t.args} state={t.state} result={t.result} meta={t.meta} />
            ))}
          </div>
        )}
      </div>

      {!atBottom && hasContent && (
        <Tip text={t("jump.latest")}>
          <button
            onClick={scrollDown}
            className="absolute bottom-4 left-1/2 flex h-8 w-8 -translate-x-1/2 items-center justify-center rounded-full border border-line bg-raised text-dim shadow-[0_4px_16px_rgba(0,0,0,0.4)] transition-colors hover:text-ink"
          >
            <ArrowDown size={14} />
          </button>
        </Tip>
      )}

      {status && (
        <div className="absolute bottom-3 left-4 flex items-center gap-2 font-mono text-[11px]">
          <Spinner size={12} />
          <span className="shimmer">{STATUS_LABEL[status] ? t(STATUS_LABEL[status]) : t("status.working")}</span>
        </div>
      )}

      {error && (
        <div className="mx-auto mb-2 w-full max-w-3xl px-4">
          <div className="flex items-start gap-2.5 rounded-xl border border-err/40 bg-err/5 px-3.5 py-2.5">
            <span className="mt-0.5 font-mono text-[10px] font-semibold uppercase tracking-wide text-err">error</span>
            <p className="min-w-0 flex-1 break-words text-[12.5px] leading-relaxed text-dim">{error}</p>
            <button onClick={() => dismissError(activeChatId!)} className="mt-0.5 text-faint transition-colors hover:text-ink">
              <X size={13} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
