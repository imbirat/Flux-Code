import { Modal } from "./ui";

/**
 * Minimal confirmation dialog: message + Cancel / Confirm row. Used for
 * destructive actions (delete session, remove project).
 */
export default function ConfirmModal({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal open={open} onClose={onCancel}>
      <div className="p-5">
        <div className="text-[14px] font-semibold text-ink">{title}</div>
        <p className="mt-2 text-[13px] leading-relaxed text-dim">{message}</p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="h-8 rounded-lg border border-line px-3 text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            {cancelLabel}
          </button>
          <button
            onClick={onConfirm}
            className={`h-8 rounded-lg px-3 text-[13px] font-medium transition-colors ${
              danger
                ? "bg-[var(--c-err)] text-white hover:opacity-90"
                : "bg-accent text-white hover:opacity-90"
            }`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </Modal>
  );
}
