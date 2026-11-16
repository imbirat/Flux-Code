import type { ChatSummary, ProjectInfo } from "./types";

/**
 * Shared session-search + grouping logic. Used by the Projects/Sessions home
 * page and the top-bar search popup so both behave identically.
 */

/** Human-readable relative timestamp for last activity. */
export function relativeTime(ts: number, now: number = Date.now()): string {
  const diff = now - ts;
  const s = Math.floor(diff / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  const w = Math.floor(d / 7);
  if (w < 5) return `${w}w ago`;
  return new Date(ts).toLocaleDateString();
}

/** Bucket key for a chat: `session.today`, `session.yesterday`, or a formatted date. */
export function dayBucket(ts: number, now: number = Date.now()): string {
  const d = new Date(ts);
  const today = new Date(now);
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  if (ts >= startOfToday) return "session.today";
  if (ts >= startOfToday - 86400_000) return "session.yesterday";
  if (now - ts < 7 * 86400_000) return d.toLocaleDateString(undefined, { weekday: "long" });
  const sameYear = d.getFullYear() === today.getFullYear();
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: sameYear ? undefined : "numeric",
  });
}

export interface GroupedSessions {
  /** `session.today`/`session.yesterday` key, or an already-formatted date. */
  label: string;
  sessions: ChatSummary[];
}

/**
 * Group sessions into ordered date buckets (newest first). Expects chats
 * already sorted by `updatedAt` descending. Group labels that start with
 * `session.` are i18n keys and should be passed through `t()`.
 */
export function groupByDate(chats: ChatSummary[], now: number = Date.now()): GroupedSessions[] {
  const groups: GroupedSessions[] = [];
  for (const c of chats) {
    const label = dayBucket(c.updatedAt, now);
    const last = groups[groups.length - 1];
    if (last && last.label === label) {
      last.sessions.push(c);
    } else {
      groups.push({ label, sessions: [c] });
    }
  }
  return groups;
}

/** Resolve a group label to display text (translates i18n keys). */
export function groupLabel(label: string, t: (key: string) => string): string {
  return label.startsWith("session.") ? t(label) : label;
}

function normalize(s: string): string {
  return s.toLowerCase().trim();
}

/** Project lookup map keyed by project id. */
function projectMap(projects: ProjectInfo[]): Map<string, ProjectInfo> {
  return new Map(projects.map((p) => [p.id, p]));
}

/**
 * Filter sessions by a free-text query across title, project name and
 * workspace path. Empty/whitespace query returns everything. Case-insensitive,
 * no extra DB calls — operates on the already-loaded chat summaries.
 */
export function searchSessions(
  chats: ChatSummary[],
  query: string,
  projects: ProjectInfo[],
): ChatSummary[] {
  const q = normalize(query);
  const byId = projectMap(projects);
  if (!q) return [...chats];
  const seen = new Set<string>();
  const out: ChatSummary[] = [];
  for (const c of chats) {
    if (seen.has(c.id)) continue;
    const project = c.projectId ? byId.get(c.projectId) : undefined;
    const haystack = normalize(
      [c.title, project?.name ?? "", c.workspace].filter(Boolean).join("\n"),
    );
    if (haystack.includes(q)) {
      seen.add(c.id);
      out.push(c);
    }
  }
  return out;
}

/** Sort by last activity, newest first (defensive copy). */
export function byRecent(chats: ChatSummary[]): ChatSummary[] {
  return [...chats].sort((a, b) => b.updatedAt - a.updatedAt);
}

/** Sessions belonging to a project (or all when projectId is null). */
export function forProject(chats: ChatSummary[], projectId: string | null): ChatSummary[] {
  if (!projectId) return [...chats];
  return chats.filter((c) => c.projectId === projectId);
}

/** Display name for a chat's project. */
export function projectNameOf(chat: ChatSummary, projects: ProjectInfo[]): string {
  const p = chat.projectId ? projects.find((x) => x.id === chat.projectId) : undefined;
  return p?.name ?? "";
}
