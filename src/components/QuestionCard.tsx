import { useState } from "react";
import { motion } from "motion/react";
import { Check, Circle, SquareCheck } from "lucide-react";
import { useStore } from "../lib/store";
import { EASE } from "../lib/motion";
import type { QuestionDef } from "../lib/types";

function One({
  q,
  value,
  onChange,
  locked,
}: {
  q: QuestionDef;
  value: string | string[] | undefined;
  onChange: (v: string | string[]) => void;
  locked: boolean;
}) {
  const t = useStore((s) => s.t);
  return (
    <div className="py-3 first:pt-1">
      <div className="flex items-baseline gap-2">
        <span className="text-[13px] font-medium">{q.question}</span>
        {q.optional && <span className="font-mono text-[10px] text-faint">{t("question.optional")}</span>}
      </div>
      {q.type === "text" ? (
        <input
          disabled={locked}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          className="mt-2 h-8 w-full rounded-lg border border-line bg-bg px-2.5 text-[13px] outline-none transition-colors focus:border-line2"
        />
      ) : (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {(q.options ??
            (q.type === "yesno" ? ["Yes", "No"] : [])).map((opt) => {
            const multi = q.type === "multi";
            const selected = multi ? ((value as string[]) ?? []).includes(opt) : value === opt;
            return (
              <button
                key={opt}
                disabled={locked}
                onClick={() => {
                  if (multi) {
                    const arr = (value as string[]) ?? [];
                    onChange(selected ? arr.filter((x) => x !== opt) : [...arr, opt]);
                  } else {
                    onChange(opt);
                  }
                }}
                className={`flex h-7 items-center gap-1.5 rounded-lg border px-3 text-[12.5px] transition-colors ${
                  selected
                    ? "border-transparent bg-accentdim text-ink"
                    : "border-line text-dim hover:bg-hover hover:text-ink"
                } ${locked && !selected ? "opacity-60" : ""}`}
              >
                {selected ? <Check size={12} /> : multi ? <SquareCheck size={12} /> : <Circle size={12} />}
                {opt}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default function QuestionCard() {
  const t = useStore((s) => s.t);
  const question = useStore((s) => s.question);
  const respond = useStore((s) => s.respondQuestion);
  const [values, setValues] = useState<Record<string, string | string[]>>({});
  if (!question) return null;

  const setVal = (id: string, v: string | string[]) => setValues((prev) => ({ ...prev, [id]: v }));

  return (
    <motion.div
      initial={{ opacity: 0, y: 10, scale: 0.99 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 0.22, ease: EASE }}
      className="mx-auto my-2 w-full max-w-3xl px-4"
    >
      <div className="overflow-hidden rounded-xl border border-line2 bg-raised shadow-[0_8px_28px_rgba(0,0,0,0.35)]">
        <div className="border-b border-line px-4 py-2.5 text-[13px] font-medium">{t("status.asking")}</div>
        <div className="divide-y divide-line px-4 py-1">
          {question.questions.map((q) => (
            <One key={q.id} q={q} value={values[q.id]} onChange={(v) => setVal(q.id, v)} locked={false} />
          ))}
        </div>
        <div className="flex items-center justify-end border-t border-line px-4 py-2.5">
          <button
            onClick={() => {
              const answers: Record<string, string | string[]> = {};
              for (const q of question.questions) {
                const v = values[q.id];
                if (v == null || v === "" || (Array.isArray(v) && v.length === 0)) continue;
                answers[q.id] = v;
              }
              respond(answers);
            }}
            className="h-8 rounded-lg bg-ink px-3.5 text-[12.5px] font-medium text-bg transition-opacity hover:opacity-90"
          >
            {t("question.submit")}
          </button>
        </div>
      </div>
    </motion.div>
  );
}
