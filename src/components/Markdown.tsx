import { memo, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import hljsPowershell from "highlight.js/lib/languages/powershell";
import { Check, Copy } from "lucide-react";
import { api } from "../lib/tauri";

function extractText(node: any): string {
  if (node == null) return "";
  if (typeof node === "string") return node;
  if (typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (typeof node === "object" && node.props) return extractText(node.props.children);
  return "";
}

function CodePre(props: any) {
  const [copied, setCopied] = useState(false);
  const { children, ...rest } = props;
  const child = Array.isArray(children) ? children[0] : children;
  const cls: string = child?.props?.className ?? "";
  const lang = cls.startsWith("language-") ? cls.slice(9) : "";
  const raw = extractText(child?.props?.children);
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
    <div className="group relative">
      <div className="absolute right-2 top-2 z-10 flex items-center gap-2 opacity-0 transition-opacity group-hover:opacity-100">
        {lang && <span className="font-mono text-[10px] text-faint">{lang}</span>}
        <button
          onClick={copy}
          title="Copy"
          className="flex h-6 w-6 items-center justify-center rounded-md border border-line bg-raised text-dim transition-colors hover:text-ink"
        >
          {copied ? <Check size={12} /> : <Copy size={12} />}
        </button>
      </div>
      <pre {...rest}>{children}</pre>
    </div>
  );
}

const components = {
  pre: CodePre,
  a: ({ href, children }: { href?: string; children: ReactNode }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (href) api.openExternal(href).catch(() => {});
      }}
    >
      {children}
    </a>
  ),
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
