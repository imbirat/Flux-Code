import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useStore } from "./lib/store";
import TopBar from "./components/TopBar";
import ProjectsHome from "./components/ProjectsHome";
import Sidebar from "./components/Sidebar";
import ChatView from "./components/ChatView";
import Composer from "./components/Composer";
import RightPanel from "./components/RightPanel";
import PermissionCard from "./components/PermissionCard";
import QuestionCard from "./components/QuestionCard";
import Settings from "./components/Settings";
import { FluxMark } from "./components/ui";
import { EASE, D } from "./lib/motion";

const EVENTS = [
  "chat:start",
  "chat:delta",
  "chat:message",
  "chat:tool",
  "chat:status",
  "chat:done",
  "chat:error",
  "chat:title",
  "todo:update",
  "permission:request",
  "question:request",
  "terminal:chunk",
  "preview:server",
  "process:exit",
];

function resolveTheme(theme: string): "dark" | "light" {
  if (theme === "dark" || theme === "light") return theme;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function Splash() {
  return (
    <div className="flex h-full flex-col items-center justify-center bg-bg">
      <motion.div
        initial={{ opacity: 0, scale: 0.96 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.3, ease: [0.25, 0.8, 0.25, 1] }}
      >
        <FluxMark size={36} />
      </motion.div>
    </div>
  );
}

/** Transient global notice (project add failures, etc.). */
function Toast() {
  const toast = useStore((s) => s.toast);
  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => useStore.getState().setToast(null), 4000);
    return () => window.clearTimeout(id);
  }, [toast]);
  return (
    <AnimatePresence>
      {toast && (
        <motion.div
          initial={{ opacity: 0, y: 8, x: "-50%" }}
          animate={{ opacity: 1, y: 0, x: "-50%" }}
          exit={{ opacity: 0, y: 4, x: "-50%" }}
          transition={{ duration: D.base, ease: EASE }}
          className="fixed bottom-4 left-1/2 z-[120] max-w-[70vw] truncate rounded-lg border border-line bg-raised px-3.5 py-2 text-[12.5px] text-ink shadow-[0_12px_32px_rgba(0,0,0,0.45)]"
          data-testid="toast"
        >
          {toast}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

export default function App() {
  const booted = useStore((s) => s.booted);
  const view = useStore((s) => s.view);
  const settings = useStore((s) => s.settings);
  const init = useStore((s) => s.init);
  const onEvent = useStore((s) => s.onEvent);
  const startNewChat = useStore((s) => s.startNewChat);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const setLeftOpen = useStore((s) => s.setLeftOpen);
  const setRightOpen = useStore((s) => s.setRightOpen);

  useEffect(() => {
    init().catch(() => {});
    const unlisteners: Promise<() => void>[] = EVENTS.map((name) =>
      listen(name, (e) => onEvent(name, e.payload)).catch(() => (() => {}) as () => void),
    );
    return () => {
      for (const p of unlisteners) p.then((fn) => fn()).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // theme + accent + reduced motion on <html>
  useEffect(() => {
    const html = document.documentElement;
    const apply = () => {
      html.dataset.theme = resolveTheme(settings.theme);
      html.dataset.accent = settings.accent;
      html.dataset.reducedMotion = String(settings.reducedMotion);
    };
    apply();
    if (settings.theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => {
      html.dataset.theme = resolveTheme(settings.theme);
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [settings.theme, settings.accent, settings.reducedMotion]);

  // global shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === "n") {
          e.preventDefault();
          startNewChat().catch(() => {});
        } else if (k === ",") {
          e.preventDefault();
          setSettingsOpen(true);
        } else if (k === "b") {
          e.preventDefault();
          setLeftOpen(!useStore.getState().leftOpen);
        } else if (k === "j") {
          e.preventDefault();
          setRightOpen(!useStore.getState().rightOpen);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [startNewChat, setSettingsOpen, setLeftOpen, setRightOpen]);

  // Startup shows the Projects/Sessions home. No session is created or opened
  // automatically — the user decides (home page, search popup or main menu).

  if (!booted) return <Splash />;

  return (
    <MotionConfig reducedMotion={settings.reducedMotion ? "always" : "user"}>
      <div className="flex h-full flex-col overflow-hidden bg-bg text-ink">
        <TopBar />
        <div className="flex min-h-0 flex-1 overflow-hidden">
          {view === "home" ? (
            <ProjectsHome />
          ) : (
            <>
              <Sidebar />
              <main className="flex min-w-0 flex-1 flex-col">
                <ChatView />
                <PermissionCard />
                <QuestionCard />
                <Composer />
              </main>
              <RightPanel />
            </>
          )}
        </div>
        <Settings />
        <Toast />
      </div>
    </MotionConfig>
  );
}
