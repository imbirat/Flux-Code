import { describe, expect, it } from "vitest";
import {
  byRecent,
  dayBucket,
  forProject,
  groupByDate,
  groupLabel,
  projectNameOf,
  relativeTime,
  searchSessions,
} from "../sessions";
import type { ChatSummary, ProjectInfo } from "../types";

const NOW = new Date("2026-03-15T12:00:00").getTime();
const H = 3600_000;

function chat(partial: Partial<ChatSummary> & { id: string }): ChatSummary {
  return {
    title: "Session",
    workspace: "",
    provider: "",
    model: "",
    effort: "",
    permissionMode: "ask",
    createdAt: 0,
    updatedAt: NOW,
    projectId: null,
    ...partial,
  };
}

const projects: ProjectInfo[] = [
  { id: "p1", name: "Alpha", path: "C:\\code\\alpha", sort: 0, createdAt: 0 },
  { id: "p2", name: "Beta", path: "C:\\code\\beta", sort: 1, createdAt: 0 },
];

describe("relativeTime", () => {
  it("formats sub-minute activity as just now", () => {
    expect(relativeTime(NOW - 30_000, NOW)).toBe("just now");
  });
  it("formats minutes and hours", () => {
    expect(relativeTime(NOW - 5 * 60_000, NOW)).toBe("5m ago");
    expect(relativeTime(NOW - 3 * H, NOW)).toBe("3h ago");
  });
  it("formats days and weeks", () => {
    expect(relativeTime(NOW - 2 * 24 * H, NOW)).toBe("2d ago");
    expect(relativeTime(NOW - 14 * 24 * H, NOW)).toBe("2w ago");
  });
});

describe("dayBucket / groupByDate", () => {
  it("buckets today, yesterday and older dates", () => {
    expect(dayBucket(NOW, NOW)).toBe("session.today");
    expect(dayBucket(NOW - 2 * H, NOW)).toBe("session.today");
    expect(dayBucket(NOW - 20 * H, NOW)).toBe("session.yesterday");
    // 3 days ago -> formatted date, not a weekday key.
    expect(dayBucket(NOW - 3 * 24 * H, NOW)).not.toMatch(/^session\./);
  });

  it("groups consecutive sessions into ordered buckets", () => {
    const chats = [
      chat({ id: "a", updatedAt: NOW - 1 * H }),
      chat({ id: "b", updatedAt: NOW - 2 * H }),
      chat({ id: "c", updatedAt: NOW - 20 * H }),
      chat({ id: "d", updatedAt: NOW - 30 * 24 * H }),
    ];
    const groups = groupByDate(chats, NOW);
    expect(groups.map((g) => g.label)).toEqual([
      "session.today",
      "session.yesterday",
      expect.any(String),
    ]);
    expect(groups[0].sessions.map((s) => s.id)).toEqual(["a", "b"]);
    expect(groups[1].sessions.map((s) => s.id)).toEqual(["c"]);
    expect(groups[2].sessions.map((s) => s.id)).toEqual(["d"]);
  });

  it("groupLabel translates i18n keys and passes dates through", () => {
    const t = (k: string) => (k === "session.today" ? "Heute" : k);
    expect(groupLabel("session.today", t)).toBe("Heute");
    expect(groupLabel("March 12", t)).toBe("March 12");
  });
});

describe("searchSessions", () => {
  const chats = [
    chat({ id: "1", title: "Fix login bug", projectId: "p1", workspace: "C:\\code\\alpha" }),
    chat({ id: "2", title: "Write README", projectId: "p2", workspace: "C:\\code\\beta" }),
    chat({ id: "3", title: "Refactor db", projectId: null, workspace: "C:\\other" }),
  ];

  it("returns everything for an empty query", () => {
    expect(searchSessions(chats, "  ", projects)).toHaveLength(3);
  });

  it("matches by title (case-insensitive)", () => {
    expect(searchSessions(chats, "LOGIN", projects).map((c) => c.id)).toEqual(["1"]);
  });

  it("matches by project name", () => {
    expect(searchSessions(chats, "alpha", projects).map((c) => c.id)).toEqual(["1"]);
  });

  it("matches by workspace path", () => {
    expect(searchSessions(chats, "c:\\other", projects).map((c) => c.id)).toEqual(["3"]);
  });

  it("never returns duplicates", () => {
    // Title and project name both contain "a" — id must still appear once.
    const out = searchSessions(chats, "a", projects);
    expect(new Set(out.map((c) => c.id)).size).toBe(out.length);
  });

  it("returns nothing when nothing matches", () => {
    expect(searchSessions(chats, "nonexistent", projects)).toEqual([]);
  });
});

describe("byRecent / forProject / projectNameOf", () => {
  const chats = [
    chat({ id: "old", updatedAt: NOW - 10 * H, projectId: "p1" }),
    chat({ id: "new", updatedAt: NOW, projectId: "p2" }),
  ];

  it("sorts newest first without mutating the input", () => {
    const sorted = byRecent(chats);
    expect(sorted.map((c) => c.id)).toEqual(["new", "old"]);
    expect(chats[0].id).toBe("old");
  });

  it("filters by project and passes through null scope", () => {
    expect(forProject(chats, "p1").map((c) => c.id)).toEqual(["old"]);
    expect(forProject(chats, null)).toHaveLength(2);
  });

  it("resolves project display names", () => {
    expect(projectNameOf(chats[0], projects)).toBe("Alpha");
    expect(projectNameOf(chat({ id: "x", projectId: "zz" }), projects)).toBe("");
  });
});
