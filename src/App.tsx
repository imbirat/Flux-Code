import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig, motion } from "motion/react";
import { useStore } from "./lib/store";
import Sidebar from "./components/Sidebar";
import ChatView from "./components/ChatView";
import Composer from "./components/Composer";
import RightPanel from "./components/RightPanel";
import PermissionCard from "./components/PermissionCard";
import QuestionCard from "./components/QuestionCard";
import Settings from "./components/Settings";
import { FluxMark } from "./components/ui";

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

export default function App() {
  const booted = useStore((s) => s.booted);
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

  // ensure a chat exists once booted
  useEffect(() => {
    if (booted && !useStore.getState().activeChatId) startNewChat().catch(() => {});
  }, [booted, startNewChat]);

  if (!booted) return <Splash />;

  return (
    <MotionConfig reducedMotion={settings.reducedMotion ? "always" : "user"}>
      <div className="flex h-full overflow-hidden bg-bg text-ink">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <ChatView />
          <PermissionCard />
          <QuestionCard />
          <Composer />
        </main>
        <RightPanel />
        <Settings />
      </div>
    </MotionConfig>
  );
}
