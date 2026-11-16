import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Check, Circle, CircleDot, ListTodo, Plus, X } from "lucide-react";
import { useStore } from "../lib/store";
import { EASE, listItem } from "../lib/motion";
import type { TodoItem } from "../lib/types";

export default function TodoPanel() {
  const t = useStore((s) => s.t);
  const activeChatId = useStore((s) => s.activeChatId);
  const todos = useStore((s) => (s.activeChatId ? s.todos[s.activeChatId] : undefined)) ?? [];
  const setTodos = useStore((s) => s.setTodos);
  const [draft, setDraft] = useState("");

  const done = todos.filter((x) => x.status === "completed").length;
  const total = todos.length;

  const toggle = (item: TodoItem) => {
    if (!activeChatId) return;
    setTodos(
      activeChatId,
      todos.map((x) => (x.id === item.id ? { ...x, status: x.status === "completed" ? "pending" : "completed" } : x)),
    ).catch(() => {});
  };

  const remove = (item: TodoItem) => {
    if (!activeChatId) return;
    setTodos(activeChatId, todos.filter((x) => x.id !== item.id)).catch(() => {});
  };

  const add = () => {
    if (!draft.trim() || !activeChatId) return;
    setTodos(activeChatId, [
      ...todos,
      { id: `u${Date.now().toString(36)}`, content: draft.trim(), status: "pending", sort: todos.length },
    ]).catch(() => {});
    setDraft("");
  };

  return (
    <div className="flex h-full flex-col">
      {total > 0 && (
        <div className="flex-none px-3 pb-1.5 pt-3">
          <div className="mb-1.5 flex items-baseline justify-between">
            <span className="text-[11px] text-faint">{t("todo.completed")}</span>
            <span className="font-mono text-[10.5px] tabular-nums text-faint">
              {done}/{total}
            </span>
          </div>
          <div className="h-1 overflow-hidden rounded-full bg-hover">
            <motion.div
              className="h-full rounded-full bg-ok/70"
              initial={false}
              animate={{ width: `${total ? (done / total) * 100 : 0}%` }}
              transition={{ duration: 0.3, ease: EASE }}
            />
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {todos.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center px-6 text-center">
            <ListTodo size={22} className="text-faint" />
            <p className="mt-3 max-w-52 text-[12px] leading-relaxed text-faint">{t("todo.empty")}</p>
          </div>
        ) : (
          <ul className="space-y-0.5">
            <AnimatePresence initial={false}>
              {todos.map((item) => {
                const isDone = item.status === "completed";
                const active = item.status === "in_progress";
                return (
                  <motion.li
                    key={item.id}
                    layout="position"
                    variants={listItem}
                    initial="initial"
                    animate="animate"
                    exit="exit"
                    className="group flex items-start gap-2 rounded-lg px-2 py-1.5 transition-colors hover:bg-hover/60"
                  >
                    <button
                      onClick={() => toggle(item)}
                      className="mt-0.5 shrink-0 text-faint transition-colors hover:text-accent"
                      aria-label={isDone ? "Mark pending" : "Mark completed"}
                    >
                      <AnimatePresence mode="wait" initial={false}>
                        {isDone ? (
                          <motion.span
                            key="done"
                            initial={{ scale: 0.6, opacity: 0 }}
                            animate={{ scale: 1, opacity: 1 }}
                            exit={{ scale: 0.7, opacity: 0 }}
                            transition={{ duration: 0.15, ease: EASE }}
                            className="flex"
                          >
                            <Circle size={14} className="fill-accent text-accent" />
                          </motion.span>
                        ) : active ? (
                          <motion.span key="active" initial={{ scale: 0.8, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0.8, opacity: 0 }} transition={{ duration: 0.15, ease: EASE }} className="flex">
                            <CircleDot size={14} className="text-accent" />
                          </motion.span>
                        ) : (
                          <motion.span key="pending" initial={{ scale: 0.8, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0.8, opacity: 0 }} transition={{ duration: 0.15, ease: EASE }} className="flex">
                            <Circle size={14} />
                          </motion.span>
                        )}
                      </AnimatePresence>
                    </button>
                    <motion.span
                      className={`min-w-0 flex-1 break-words text-[12.5px] leading-relaxed ${
                        isDone ? "text-faint line-through" : active ? "text-ink" : "text-dim"
                      }`}
                    >
                      {item.content}
                    </motion.span>
                    {isDone && (
                      <motion.span
                        initial={{ scale: 0.5, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        transition={{ duration: 0.16, ease: EASE }}
                        className="mt-0.5 flex-none text-ok"
                      >
                        <Check size={12} />
                      </motion.span>
                    )}
                    <button
                      onClick={() => remove(item)}
                      className="mt-0.5 shrink-0 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-ink"
                      aria-label="Remove task"
                    >
                      <X size={12} />
                    </button>
                  </motion.li>
                );
              })}
            </AnimatePresence>
          </ul>
        )}
      </div>
      <div className="flex-none border-t border-line p-2.5">
        <div className="flex h-8 items-center gap-2 rounded-lg border border-line bg-bg px-2.5 transition-colors focus-within:border-line2">
          <Plus size={13} className="shrink-0 text-faint" />
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") add();
            }}
            placeholder={t("todo.add")}
            className="min-w-0 flex-1 bg-transparent text-[12.5px] outline-none placeholder:text-faint"
          />
        </div>
      </div>
    </div>
  );
}
