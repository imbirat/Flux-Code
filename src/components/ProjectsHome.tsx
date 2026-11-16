import { useEffect, useMemo, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import {
  Folder,
  LayoutGrid,
  MessageSquare,
  Pencil,
  Plus,
  Search,
  Settings as SettingsIcon,
  Trash2,
  X,
  CircleHelp,
} from "lucide-react";
import { useStore } from "../lib/store";
import {
  byRecent,
  forProject,
  groupByDate,
  groupLabel,
  relativeTime,
  searchSessions,
} from "../lib/sessions";
import { pickAndAddProject, projectNameOrFallback } from "../lib/projects";
import { useDebounced } from "../lib/useDebounced";
import { api } from "../lib/tauri";
import { fadeRise } from "../lib/motion";
import ConfirmModal from "./ConfirmModal";
import type { ChatSummary, ProjectInfo } from "../lib/types";
import type { TFn } from "../lib/i18n";

const REPO_HELP_URL = "https://github.com/imbirat/Flux-Code#readme";

/**
 * Projects/Sessions home page (top button 2). Left: project rail. Main:
 * search + New Session, sessions grouped by date. Real data only — projects
 * and sessions come from the database.
 */
export default function ProjectsHome() {
  const t: TFn = useStore((s) => s.t);
  const projects = useStore((s) => s.projects);
  const activeProjectId = useStore((s) => s.activeProjectId);
  const setActiveProject = useStore((s) => s.setActiveProject);
  const renameProject = useStore((s) => s.renameProject);
  const removeProject = useStore((s) => s.removeProject);
  const chats = useStore((s) => s.chats);
  const activeChatId = useStore((s) => s.activeChatId);
  const openChat = useStore((s) => s.openChat);
  const startNewChat = useStore((s) => s.startNewChat);
  const renameChat = useStore((s) => s.renameChat);
  const deleteChat = useStore((s) => s.deleteChat);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  const [q, setQ] = useState("");
  const dq = useDebounced(q, 120);
  const [editingProject, setEditingProject] = useState<string | null>(null);
  const [projectDraft, setProjectDraft] = useState("");
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [sessionDraft, setSessionDraft] = useState("");
  const [sessionMenuFor, setSessionMenuFor] = useState<string | null>(null);
  const [confirmSession, setConfirmSession] = useState<ChatSummary | null>(null);
  const [confirmProject, setConfirmProject] = useState<ProjectInfo | null>(null);

  const scoped = useMemo(() => forProject(chats, activeProjectId), [chats, activeProjectId]);
  const results = useMemo(
    () => byRecent(searchSessions(scoped, dq, projects)),
    [scoped, dq, projects],
  );
  const groups = useMemo(() => groupByDate(results), [results]);

  // Keep the confirm dialog's data fresh if the underlying chat disappears.
  useEffect(() => {
    if (!confirmSession) return;
    if (!chats.some((c) => c.id === confirmSession.id)) setConfirmSession(null);
  }, [chats, confirmSession]);

  const noProjects = projects.length === 0;

  return (
    <div className="flex min-h-0 w-full flex-1 overflow-hidden">
      {/* ---------- left rail: projects ---------- */}
      <aside
        className="flex w-60 flex-none flex-col border-r border-line bg-panel"
        data-testid="projects-rail"
      >
        <div className="flex h-12 flex-none items-center justify-between px-4">
          <span className="text-[13px] font-semibold tracking-tight text-ink">{t("home.projects")}</span>
          <button
            data-testid="rail-add-project"
            onClick={() => pickAndAddProject().catch(() => {})}
            title={t("home.addProject")}
            className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            <Plus size={13} />
            <span className="hidden xl:inline">{t("home.addProject")}</span>
          </button>
        </div>

        <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="project-list">
          {noProjects && (
            <div className="px-2.5 py-4 text-[12.5px] text-faint" data-testid="projects-empty">
              {t("home.empty.projects")}
            </div>
          )}
          {/* All Projects pseudo-entry */}
          {!noProjects && (
            <button
              data-testid="project-all"
              onClick={() => setActiveProject(null)}
              className={`mt-1 flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] transition-colors ${
                activeProjectId === null ? "bg-hover text-ink" : "text-dim hover:bg-hover/60 hover:text-ink"
              }`}
            >
              <LayoutGrid size={14} className="shrink-0" />
              <span className="min-w-0 flex-1 truncate">{t("home.allProjects")}</span>
            </button>
          )}
          {projects.map((p) => {
            const active = p.id === activeProjectId;
            const isEditing = editingProject === p.id;
            return (
              <div
                key={p.id}
                className={`group/project relative flex h-9 items-center rounded-lg transition-colors ${
                  active ? "bg-hover" : "hover:bg-hover/60"
                }`}
                data-testid={`project-row-${p.id}`}
              >
                {isEditing ? (
                  <input
                    autoFocus
                    value={projectDraft}
                    onChange={(e) => setProjectDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && projectDraft.trim()) {
                        renameProject(p.id, projectDraft.trim()).catch(() => {});
                        setEditingProject(null);
                      }
                      if (e.key === "Escape") setEditingProject(null);
                    }}
                    onBlur={() => setEditingProject(null)}
                    className="mx-1.5 h-6 w-[calc(100%-12px)] rounded-md border border-line2 bg-bg px-2 text-[13px] outline-none"
                  />
                ) : (
                  <>
                    <button
                      onClick={() => setActiveProject(p.id)}
                      title={p.path}
                      className="flex h-9 min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 text-left"
                    >
                      <Folder size={14} className={active ? "shrink-0 text-accent" : "shrink-0 text-dim"} />
                      <span
                        className={`min-w-0 flex-1 truncate text-[13px] ${
                          active ? "text-ink" : "text-dim group-hover/project:text-ink"
                        }`}
                      >
                        {projectNameOrFallback(p)}
                      </span>
                    </button>
                    <div className="absolute right-1.5 hidden items-center gap-0.5 group-hover/project:flex">
                      <button
                        title={t("project.rename")}
                        onClick={() => {
                          setProjectDraft(p.name);
                          setEditingProject(p.id);
                        }}
                        className="flex h-6 w-6 items-center justify-center rounded-md text-faint transition-colors hover:bg-raised hover:text-ink"
                      >
                        <Pencil size={11} />
                      </button>
                      <button
                        title={t("project.remove")}
                        onClick={() => setConfirmProject(p)}
                        className="flex h-6 w-6 items-center justify-center rounded-md text-faint transition-colors hover:bg-raised hover:text-err"
                      >
                        <Trash2 size={11} />
                      </button>
                    </div>
                  </>
                )}
              </div>
            );
          })}
        </nav>

        <div className="flex-none border-t border-line p-2">
          <button
            data-testid="rail-settings"
            onClick={() => setSettingsOpen(true)}
            className="flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            <SettingsIcon size={14} />
            {t("home.settings")}
          </button>
          <button
            data-testid="rail-help"
            onClick={() => api.openExternal(REPO_HELP_URL).catch(() => {})}
            className="flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13px] text-dim transition-colors hover:bg-hover hover:text-ink"
          >
            <CircleHelp size={14} />
            {t("home.help")}
          </button>
        </div>
      </aside>

      {/* ---------- main: sessions ---------- */}
      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-none items-center gap-3 px-6 pb-3 pt-5">
          <div className="flex h-8 w-full max-w-[380px] items-center gap-2 rounded-lg border border-line bg-raised px-3">
            <Search size={13} className="shrink-0 text-faint" />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder={t("home.searchSessions")}
              data-testid="home-search"
              className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-ink outline-none placeholder:text-faint"
            />
          </div>
          <div className="flex-1" />
          <button
            data-testid="home-new-session"
            onClick={() => startNewChat().catch(() => {})}
            className="flex h-8 flex-none items-center gap-1.5 rounded-lg border border-line bg-raised px-3 text-[13px] text-ink transition-colors hover:bg-hover"
          >
            <Plus size={14} />
            {t("home.newSession")}
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-8" data-testid="sessions-list">
          {noProjects ? (
            <EmptyState
              icon={<Folder size={20} />}
              title={t("home.empty.projects")}
              action={
                <button
                  data-testid="empty-add-project"
                  onClick={() => pickAndAddProject().catch(() => {})}
                  className="mt-4 flex h-8 items-center gap-1.5 rounded-lg border border-line bg-raised px-3 text-[13px] text-ink transition-colors hover:bg-hover"
                >
                  <Plus size={14} />
                  {t("home.addProject")}
                </button>
              }
            />
          ) : groups.length === 0 ? (
            <EmptyState
              icon={<MessageSquare size={20} />}
              title={
                dq.trim()
                  ? t("home.empty.search")
                  : activeProjectId === null
                    ? t("home.empty.sessions")
                    : t("home.empty.sessionsFiltered")
              }
              action={
                !dq.trim() ? (
                  <button
                    data-testid="empty-new-session"
                    onClick={() => startNewChat().catch(() => {})}
                    className="mt-4 flex h-8 items-center gap-1.5 rounded-lg border border-line bg-raised px-3 text-[13px] text-ink transition-colors hover:bg-hover"
                  >
                    <Plus size={14} />
                    {t("home.newSession")}
                  </button>
                ) : undefined
              }
            />
          ) : (
            groups.map((g) => (
              <div key={g.label} className="mb-5">
                <div className="mb-1.5 px-3 text-[12px] font-medium text-faint">
                  {groupLabel(g.label, t)}
                </div>
                {g.sessions.map((c) => {
                  const isEditing = editingSession === c.id;
                  const menuOpen = sessionMenuFor === c.id;
                  const isActive = c.id === activeChatId;
                  return (
                    <div
                      key={c.id}
                      className={`group/session relative flex h-10 items-center rounded-lg transition-colors ${
                        isActive ? "bg-hover/70" : "hover:bg-hover/60"
                      }`}
                      data-testid={`session-row-${c.id}`}
                    >
                      {isEditing ? (
                        <input
                          autoFocus
                          value={sessionDraft}
                          onChange={(e) => setSessionDraft(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" && sessionDraft.trim()) {
                              renameChat(c.id, sessionDraft.trim());
                              setEditingSession(null);
                            }
                            if (e.key === "Escape") setEditingSession(null);
                          }}
                          onBlur={() => setEditingSession(null)}
                          className="mx-3 h-6 w-[calc(100%-24px)] rounded-md border border-line2 bg-bg px-2 text-[13px] outline-none"
                        />
                      ) : (
                        <>
                          <button
                            onClick={() => openChat(c.id)}
                            className="flex h-10 min-w-0 flex-1 items-center gap-3 rounded-lg px-3 text-left"
                          >
                            <MessageSquare
                              size={14}
                              className={isActive ? "shrink-0 text-accent" : "shrink-0 text-faint"}
                            />
                            <span className="min-w-0 flex-[2] truncate text-[13px] text-ink">
                              {c.title}
                            </span>
                            {activeProjectId === null && (
                              <span className="hidden min-w-0 flex-1 truncate text-[11.5px] text-faint md:block">
                                {c.projectId
                                  ? projectNameOrFallback(
                                      projects.find((p) => p.id === c.projectId) ?? {
                                        name: "",
                                        path: c.workspace,
                                      },
                                    )
                                  : ""}
                              </span>
                            )}
                            <span className="w-16 flex-none text-right font-mono text-[10.5px] text-faint">
                              {relativeTime(c.updatedAt)}
                            </span>
                          </button>
                          <div
                            className={`absolute right-2 flex items-center gap-0.5 ${
                              menuOpen ? "flex" : "hidden group-hover/session:flex"
                            }`}
                          >
                            {menuOpen ? (
                              <>
                                <button
                                  title={t("common.rename")}
                                  onClick={() => {
                                    setSessionDraft(c.title);
                                    setEditingSession(c.id);
                                    setSessionMenuFor(null);
                                  }}
                                  className="flex h-6 w-6 items-center justify-center rounded-md text-faint hover:bg-raised hover:text-ink"
                                >
                                  <Pencil size={11} />
                                </button>
                                <button
                                  title={t("common.delete")}
                                  onClick={() => {
                                    setConfirmSession(c);
                                    setSessionMenuFor(null);
                                  }}
                                  className="flex h-6 w-6 items-center justify-center rounded-md text-faint hover:bg-raised hover:text-err"
                                >
                                  <Trash2 size={11} />
                                </button>
                                <button
                                  onClick={() => setSessionMenuFor(null)}
                                  className="flex h-6 w-6 items-center justify-center rounded-md text-faint hover:bg-raised hover:text-ink"
                                >
                                  <X size={11} />
                                </button>
                              </>
                            ) : (
                              <button
                                title={t("common.rename")}
                                onClick={() => setSessionMenuFor(menuOpen ? null : c.id)}
                                className="flex h-6 w-6 items-center justify-center rounded-md text-faint hover:bg-raised hover:text-ink"
                              >
                                <span className="text-[12px] leading-none tracking-widest">···</span>
                              </button>
                            )}
                          </div>
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            ))
          )}
        </div>
      </section>

      {/* ---------- confirmations ---------- */}
      <ConfirmModal
        open={confirmSession !== null}
        title={t("common.delete")}
        message={t("session.delete.confirm").replace("{title}", confirmSession?.title ?? "")}
        confirmLabel={t("common.delete")}
        cancelLabel={t("common.cancel")}
        danger
        onConfirm={() => {
          if (confirmSession) deleteChat(confirmSession.id);
          setConfirmSession(null);
        }}
        onCancel={() => setConfirmSession(null)}
      />
      <ConfirmModal
        open={confirmProject !== null}
        title={t("project.remove")}
        message={t("project.remove.confirm").replace("{name}", confirmProject ? projectNameOrFallback(confirmProject) : "")}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        onConfirm={() => {
          if (confirmProject) removeProject(confirmProject.id).catch(() => {});
          setConfirmProject(null);
        }}
        onCancel={() => setConfirmProject(null)}
      />
    </div>
  );
}

function EmptyState({
  icon,
  title,
  action,
}: {
  icon: ReactNode;
  title: string;
  action?: ReactNode;
}) {
  return (
    <motion.div
      variants={fadeRise}
      initial="initial"
      animate="animate"
      className="flex h-full flex-col items-center justify-center text-center"
      data-testid="home-empty"
    >
      <div className="text-faint">{icon}</div>
      <div className="mt-2.5 max-w-xs text-[13px] text-dim">{title}</div>
      {action}
    </motion.div>
  );
}
