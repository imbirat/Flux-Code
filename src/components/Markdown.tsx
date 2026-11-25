import { memo, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import hljsPowershell from "highlight.js/lib/languages/powershell";
import { Check, ChevronDown, Copy, WrapText } from "lucide-react";
import { api } from "../lib/tauri";
import { safeLinkHref } from "../lib/safeUrl";

function extractText(node: any): string {
  if (node == null) return "";
  if (typeof node === "string") return node;
  if (typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (typeof node === "object" && node.props) return extractText(node.props.children);
  return "";
}

/** Lines above which a code block starts collapsed. */
const COLLAPSE_LINES = 24;

function CodePre(props: any) {
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState(true);
  const [wrap, setWrap] = useState(false);
  const { children, ...rest } = props;
  const child = Array.isArray(children) ? children[0] : children;
  const cls: string = child?.props?.className ?? "";
  const lang = cls.startsWith("language-") ? cls.slice(9) : "";
  const raw = extractText(child?.props?.children);
  const lineCount = raw ? raw.split("\n").length : 0;
  const collapsible = lineCount > COLLAPSE_LINES;
  const shownChildren = collapsible && !open ? null : children;

  const copy = () => {
    navigator.clipboard.writeText(raw).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  return (
    <div className="group my-2.5 overflow-hidden rounded-xl border border-line bg-[var(--c-code-bg)]">
      <div className="flex h-8 flex-none items-center gap-2 border-b border-line px-3">
        <span className="font-mono text-[10.5px] uppercase tracking-wide text-faint">{lang || "code"}</span>
        <span className="flex-1" />
        <button
          onClick={() => setWrap((w) => !w)}
          title={wrap ? "Disable wrap" : "Wrap lines"}
          className={`hidden h-6 w-6 items-center justify-center rounded-md text-faint opacity-0 transition-all group-hover:opacity-100 hover:bg-hover hover:text-dim sm:flex ${
            wrap ? "text-dim opacity-100" : ""
          }`}
        >
          <WrapText size={12} />
        </button>
        <button
          onClick={copy}
          title="Copy"
          className="flex h-6 w-6 items-center justify-center rounded-md border border-line bg-raised text-dim transition-colors hover:text-ink"
        >
          {copied ? <Check size={12} className="text-ok" /> : <Copy size={12} />}
        </button>
      </div>
      <pre {...rest} style={{ ...(rest.style ?? {}), whiteSpace: wrap ? "pre-wrap" : undefined }}>
        {shownChildren}
      </pre>
      {collapsible && (
        <button
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center gap-1.5 border-t border-line px-3 py-1.5 font-mono text-[10.5px] text-faint transition-colors hover:text-dim"
        >
          <ChevronDown size={11} className={`transition-transform ${open ? "" : "-rotate-90"}`} />
          {open ? "Collapse" : `Show all ${lineCount} lines`}
        </button>
      )}
    </div>
  );
}

const components = {
  pre: CodePre,
  a: ({ href, children }: { href?: string; children: ReactNode }) => {
    const safe = safeLinkHref(href);
    // file:, javascript:, data: and malformed targets are rendered as plain
    // text — the model must never be able to trigger a local path or script.
    if (!safe) return <span>{children}</span>;
    return (
      <a
        href={safe}
        onClick={(e) => {
          e.preventDefault();
          api.openExternal(safe).catch(() => {});
        }}
      >
        {children}
      </a>
    );
  },
  img: ({ src, alt }: { src?: string; alt?: string }) =>
    typeof src === "string" ? <img src={src} alt={alt ?? ""} loading="lazy" /> : null,
};

function MarkdownInner({ children }: { children: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[[rehypeKatex, { throwOnError: false }], [rehypeHighlight, { languages: { powershell: hljsPowershell } }]]}
        components={components as any}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

export const Markdown = memo(MarkdownInner);
