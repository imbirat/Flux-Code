import { useEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Loader2 } from "lucide-react";
import { backdrop, baseTransition, fastTransition, menuPop, modalPanel } from "../lib/motion";

export function FluxMark({ size = 20 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className="shrink-0">
      <rect x="0.75" y="0.75" width="22.5" height="22.5" rx="6" fill="#17171a" stroke="rgba(255,255,255,0.16)" strokeWidth="1" />
      <path d="M8.7 7.3 L14.1 12 L8.7 16.7" fill="none" stroke="#f5f5f6" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M16.6 13.2 v3.5" stroke="#f5f5f6" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function Spinner({ size = 13 }: { size?: number }) {
  return <Loader2 size={size} className="spin text-dim" />;
}

export function Menu({
  trigger,
  children,
  align = "left",
  direction = "down",
  width = 260,
}: {
  trigger: ReactNode;
  children: ReactNode;
  align?: "left" | "right";
  direction?: "up" | "down";
  width?: number;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const pos =
    direction === "up"
      ? `bottom-full mb-1.5 ${align === "right" ? "right-0" : "left-0"}`
      : `top-full mt-1.5 ${align === "right" ? "right-0" : "left-0"}`;
  return (
    <div className="relative" ref={ref}>
      <div onClick={() => setOpen((o) => !o)}>{trigger}</div>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={menuPop(direction).initial}
            animate={menuPop(direction).animate}
            exit={menuPop(direction).exit}
            className={`absolute ${pos} z-50 origin-bottom overflow-hidden rounded-xl border border-line bg-raised py-1 shadow-[0_12px_32px_rgba(0,0,0,0.45)]`}
            style={{ width }}
            onClick={() => setOpen(false)}
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export function MenuItem({
  icon,
  label,
  hint,
  active,
  onClick,
}: {
  icon?: ReactNode;
  label: ReactNode;
  hint?: ReactNode;
  active?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] transition-colors hover:bg-hover ${
        active ? "text-ink" : "text-dim"
      }`}
    >
      {icon && <span className="flex h-4 w-4 items-center justify-center shrink-0">{icon}</span>}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {hint && <span className="font-mono text-[10.5px] text-faint">{hint}</span>}
    </button>
  );
}

export function MenuDivider() {
  return <div className="my-1 border-t border-line" />;
}

export function Modal({
  open,
  onClose,
  children,
  wide,
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  return (
    <AnimatePresence>
      {open && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center p-6">
          <motion.div
            variants={backdrop}
            initial="initial"
            animate="animate"
            exit="exit"
            transition={baseTransition}
            className="absolute inset-0 bg-black/55"
            onClick={onClose}
          />
          <motion.div
            variants={modalPanel}
            initial="initial"
            animate="animate"
            exit="exit"
            transition={baseTransition}
            className={`relative flex max-h-[88vh] flex-col overflow-hidden rounded-2xl border border-line bg-panel shadow-[0_24px_64px_rgba(0,0,0,0.5)] ${
              wide ? "w-[880px] max-w-[94vw]" : "w-[480px] max-w-[94vw]"
            }`}
          >
            {children}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}

export function IconButton({
  children,
  onClick,
  title,
  active,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  title?: string;
  active?: boolean;
  className?: string;
}) {
  return (
    <motion.button
      whileTap={{ scale: 0.92 }}
      transition={fastTransition}
      title={title}
      onClick={onClick}
      className={`flex h-7 w-7 items-center justify-center rounded-lg text-dim transition-colors hover:bg-hover hover:text-ink ${
        active ? "bg-hover text-ink" : ""
      } ${className}`}
    >
      {children}
    </motion.button>
  );
}

export function Tip({ text, children }: { text: string; children: ReactNode }) {
  return (
    <span className="group/tip relative inline-flex">
      {children}
      <span className="pointer-events-none absolute bottom-full left-1/2 z-[60] mb-1.5 -translate-x-1/2 whitespace-nowrap rounded-md border border-line bg-raised px-2 py-1 font-mono text-[10.5px] text-dim opacity-0 shadow-[0_4px_12px_rgba(0,0,0,0.35)] transition-[opacity,transform] duration-150 ease-out group-hover/tip:delay-150 group-hover/tip:opacity-100 group-hover/tip:-translate-y-0.5">
        {text}
      </span>
    </span>
  );
}
