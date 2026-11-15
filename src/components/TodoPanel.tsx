import { useState } from "react";
import { Circle, CircleDot, ListTodo, Plus, X } from "lucide-react";
import { useStore } from "../lib/store";
import type { TodoItem } from "../lib/types";

export default function TodoPanel() {
  const t = useStore((s) => s.t);
  const activeChatId = useStore((s) => s.activeChatId);
  const todos = useStore((s) => (activeChatId ? s.todos[activeChatId] : undefined)) ?? [];
  const setTodos = useStore((s) => s.setTodos);
  const [draft, setDraft] = useState("");

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
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {todos.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center px-6 text-center">
            <ListTodo size={22} className="text-faint" />
            <p className="mt-3 max-w-52 text-[12px] text-faint">{t("todo.empty")}</p>
          </div>
        ) : (
          <ul className="space-y-0.5">
            {todos.map((item) => {
              const done = item.status === "completed";
              const active = item.status === "in_progress";
              return (
                <li key={item.id} className="group flex items-start gap-2 rounded-lg px-2 py-1.5 transition-colors hover:bg-hover/60">
                  <button onClick={() => toggle(item)} className="mt-0.5 shrink-0 text-faint transition-colors hover:text-accent">
                    {done ? <Circle size={14} className="fill-accent text-accent" /> : active ? <CircleDot size={14} className="text-accent" /> : <Circle size={14} />}
                  </button>
                  <span className={`min-w-0 flex-1 break-words text-[12.5px] leading-relaxed ${done ? "text-faint line-through" : active ? "text-ink" : "text-dim"}`}>
                    {item.content}
                  </span>
                  <button onClick={() => remove(item)} className="mt-0.5 shrink-0 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-ink">
                    <X size={12} />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      <div className="flex-none border-t border-line p-2.5">
        <div className="flex h-8 items-center gap-2 rounded-lg border border-line bg-bg px-2.5 focus-within:border-line2">
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
