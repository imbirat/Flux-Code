// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import ProjectsHome from "../ProjectsHome";
import { useStore } from "../../lib/store";
import type { ChatSummary, ProjectInfo, Settings } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  pickAndAddProject: vi.fn().mockResolvedValue(null),
  getMessages: vi.fn().mockResolvedValue([]),
  getTodos: vi.fn().mockResolvedValue([]),
  deleteChat: vi.fn().mockResolvedValue(undefined),
  listChats: vi.fn().mockResolvedValue([]),
  projectList: vi.fn().mockResolvedValue([]),
  projectRename: vi.fn(),
  projectRemove: vi.fn().mockResolvedValue(undefined),
  createChat: vi.fn(),
  renameChat: vi.fn().mockResolvedValue(undefined),
  openExternal: vi.fn().mockResolvedValue(undefined),
  setSettings: vi.fn().mockResolvedValue({}),
}));

vi.mock(import("../../lib/tauri"), async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return {
    ...actual,
    api: {
      ...(actual.api as unknown as Record<string, unknown>),
      getMessages: mocks.getMessages,
      getTodos: mocks.getTodos,
      deleteChat: mocks.deleteChat,
      listChats: mocks.listChats,
      projectList: mocks.projectList,
      projectRename: mocks.projectRename,
      projectRemove: mocks.projectRemove,
      createChat: mocks.createChat,
      renameChat: mocks.renameChat,
      openExternal: mocks.openExternal,
      setSettings: mocks.setSettings,
    } as unknown as typeof actual.api,
  };
});

vi.mock(import("../../lib/projects"), async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/projects")>();
  return { ...actual, pickAndAddProject: mocks.pickAndAddProject };
});

const projects: ProjectInfo[] = [
  { id: "p1", name: "Alpha", path: "C:\\code\\alpha", sort: 0, createdAt: 1 },
  { id: "p2", name: "Beta", path: "C:\\code\\beta", sort: 1, createdAt: 2 },
];

const NOW = Date.now();
const H = 3600_000;

function chat(partial: Partial<ChatSummary> & { id: string; title: string }): ChatSummary {
  return {
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

const SETTINGS: Settings = {
  theme: "dark",
  accent: "flux",
  language: "default",
  reducedMotion: false,
  workspace: "",
  extraFolders: [],
  defaultPermissionMode: "ask",
  effortPrefs: {},
  lastProvider: "",
  lastModel: "",
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listChats.mockResolvedValue([]);
  mocks.projectList.mockResolvedValue(projects);
});

afterEach(() => {
  cleanup();
  useStore.setState({
    chats: [],
    projects: [],
    activeProjectId: null,
    activeChatId: null,
    view: "home",
    toast: null,
  });
});

function seed(chats: ChatSummary[], activeProjectId: string | null = "p1") {
  useStore.setState({
    booted: true,
    settings: { ...SETTINGS },
    chats,
    projects,
    activeProjectId,
    activeChatId: null,
    view: "home",
  });
}

describe("ProjectsHome", () => {
  it("renders the project rail with saved projects and an All Projects entry", () => {
    seed([]);
    render(<ProjectsHome />);
    expect(screen.getByTestId("project-all")).toBeTruthy();
    expect(screen.getByTestId("project-row-p1")).toBeTruthy();
    expect(screen.getByTestId("project-row-p2")).toBeTruthy();
  });

  it("keeps the All Projects selection across project refreshes", async () => {
    seed([], null);
    await useStore.getState().refreshProjects();
    // A refresh must not silently jump back into the first project.
    expect(useStore.getState().activeProjectId).toBeNull();
  });

  it("persists All Projects instead of a project id", async () => {
    seed([]);
    useStore.getState().setActiveProject(null);
    await waitFor(() =>
      expect(mocks.setSettings).toHaveBeenCalledWith({ lastProjectId: "" }),
    );
  });

  it("groups sessions by date with Today/Yesterday labels", () => {
    const chats = [
      chat({ id: "today", title: "Fresh session", projectId: "p1", updatedAt: NOW - H }),
      chat({ id: "yest", title: "Older session", projectId: "p1", updatedAt: NOW - 20 * H }),
    ];
    seed(chats);
    render(<ProjectsHome />);
    expect(screen.getByText("Today")).toBeTruthy();
    expect(screen.getByText("Yesterday")).toBeTruthy();
    expect(screen.getByText("Fresh session")).toBeTruthy();
    expect(screen.getByText("Older session")).toBeTruthy();
  });

  it("shows the project name next to sessions in All Projects scope", () => {
    const chats = [chat({ id: "a", title: "T1", projectId: "p1" })];
    seed(chats, null);
    render(<ProjectsHome />);
    const list = screen.getByTestId("sessions-list");
    expect(list.textContent).toContain("Alpha");
  });

  it("opens a session on click and switches to the chat view", async () => {
    const chats = [chat({ id: "s1", title: "Session A", projectId: "p1" })];
    seed(chats);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("session-row-s1").querySelector("button")!);
    await waitFor(() => expect(mocks.getMessages).toHaveBeenCalledWith("s1"));
    await waitFor(() => expect(useStore.getState().view).toBe("chat"));
    expect(useStore.getState().activeProjectId).toBe("p1");
  });

  it("filters sessions by title with the search input", async () => {
    const chats = [
      chat({ id: "s1", title: "Fix login bug", projectId: "p1" }),
      chat({ id: "s2", title: "Write docs", projectId: "p1" }),
    ];
    seed(chats);
    render(<ProjectsHome />);
    fireEvent.change(screen.getByTestId("home-search"), { target: { value: "login" } });
    await waitFor(() => {
      const list = screen.getByTestId("sessions-list");
      expect(list.querySelectorAll('[data-testid^="session-row-"]')).toHaveLength(1);
    });
    expect(screen.getByTestId("sessions-list").textContent).toContain("Fix login bug");
  });

  it("shows the filtered empty state when nothing matches", async () => {
    const chats = [chat({ id: "s1", title: "Fix login bug", projectId: "p1" })];
    seed(chats);
    render(<ProjectsHome />);
    fireEvent.change(screen.getByTestId("home-search"), { target: { value: "zzz" } });
    await waitFor(() => expect(screen.getByTestId("home-empty")).toBeTruthy());
    expect(screen.getByTestId("sessions-list").textContent).toContain(
      "No sessions match your search",
    );
  });

  it("shows the per-project empty state when the project has no sessions", () => {
    seed([]);
    useStore.setState({ activeProjectId: "p2" });
    render(<ProjectsHome />);
    expect(screen.getByTestId("home-empty")).toBeTruthy();
    expect(screen.getByTestId("sessions-list").textContent).toContain(
      "No sessions in this project",
    );
  });

  it("New Session creates a chat in the active project and switches views", async () => {
    mocks.createChat.mockResolvedValue(
      chat({ id: "newc", title: "New chat", projectId: "p1", workspace: "C:\\code\\alpha" }),
    );
    seed([]);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("home-new-session"));
    await waitFor(() =>
      expect(mocks.createChat).toHaveBeenCalledWith(
        expect.objectContaining({ projectId: "p1", workspace: "C:\\code\\alpha" }),
      ),
    );
    await waitFor(() => expect(useStore.getState().view).toBe("chat"));
  });

  it("deletes a session after confirmation", async () => {
    mocks.listChats.mockResolvedValue([]);
    const chats = [chat({ id: "s1", title: "Doomed session", projectId: "p1" })];
    seed(chats);
    render(<ProjectsHome />);
    // Reveal the row actions (···), open the delete confirm, confirm.
    fireEvent.click(screen.getByTestId("session-row-s1").querySelector('[title="Rename"]')!);
    fireEvent.click(screen.getByTestId("session-row-s1").querySelector('[title="Delete"]')!);
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Doomed session");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(mocks.deleteChat).toHaveBeenCalledWith("s1"));
  });

  it("renames a project inline", async () => {
    mocks.projectRename.mockResolvedValue({
      id: "p1",
      name: "Alpha Prime",
      path: "C:\\code\\alpha",
      sort: 0,
      createdAt: 1,
    });
    seed([]);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("project-row-p1").querySelector('[title="Rename project"]')!);
    const input = screen
      .getByTestId("project-row-p1")
      .querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Alpha Prime" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(mocks.projectRename).toHaveBeenCalledWith("p1", "Alpha Prime"));
    await waitFor(() =>
      expect(screen.getByTestId("project-row-p1").textContent).toContain("Alpha Prime"),
    );
  });

  it("removes a project after confirmation and keeps its sessions", async () => {
    const chats = [chat({ id: "s1", title: "Kept session", projectId: "p1" })];
    mocks.listChats.mockResolvedValue(chats.map((c) => ({ ...c, projectId: null })));
    mocks.projectList.mockResolvedValue([projects[1]]);
    seed(chats);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("project-row-p1").querySelector('[title="Remove project"]')!);
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Remove project");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(mocks.projectRemove).toHaveBeenCalledWith("p1"));
    await waitFor(() => expect(useStore.getState().projects.map((p) => p.id)).toEqual(["p2"]));
    // The session survives — unlinked from the removed project, not deleted.
    await waitFor(() => {
      const kept = useStore.getState().chats;
      expect(kept).toHaveLength(1);
      expect(kept[0].projectId).toBeNull();
    });
  });

  it("Add Project buttons go through the folder picker", async () => {
    seed([]);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("rail-add-project"));
    await waitFor(() => expect(mocks.pickAndAddProject).toHaveBeenCalledTimes(1));
  });

  it("opens Settings from the rail without leaving data behind", () => {
    seed([]);
    render(<ProjectsHome />);
    fireEvent.click(screen.getByTestId("rail-settings"));
    expect(useStore.getState().settingsOpen).toBe(true);
  });
});
