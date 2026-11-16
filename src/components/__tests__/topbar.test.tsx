// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import TopBar from "../TopBar";
import { useStore } from "../../lib/store";
import type { ChatSummary, ProjectInfo, Settings } from "../../lib/types";

// Hoisted so the vi.mock factories can reference them without a TDZ error.
const mocks = vi.hoisted(() => ({
  isTauri: true,
  win: {
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(() => {}),
  },
  pickAndAddProject: vi.fn().mockResolvedValue(null),
  createChat: vi.fn(),
  getMessages: vi.fn().mockResolvedValue([]),
  getTodos: vi.fn().mockResolvedValue([]),
  setSettings: vi.fn().mockResolvedValue({}),
  openExternal: vi.fn().mockResolvedValue(undefined),
}));

vi.mock(import("../../lib/tauri"), async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return {
    ...actual,
    isTauri: mocks.isTauri,
    win: mocks.win,
    api: {
      ...(actual.api as unknown as Record<string, unknown>),
      createChat: mocks.createChat,
      getMessages: mocks.getMessages,
      getTodos: mocks.getTodos,
      setSettings: mocks.setSettings,
      openExternal: mocks.openExternal,
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

function chat(partial: Partial<ChatSummary> & { id: string; title: string }): ChatSummary {
  return {
    workspace: "",
    provider: "",
    model: "",
    effort: "",
    permissionMode: "ask",
    createdAt: 0,
    updatedAt: Date.now(),
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

function seedStore(chats: ChatSummary[]) {
  useStore.setState({
    booted: true,
    settings: { ...SETTINGS },
    chats,
    projects,
    activeProjectId: "p1",
    activeChatId: null,
    view: "home",
    toast: null,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.createChat.mockImplementation((p: { workspace: string }) =>
    Promise.resolve(
      chat({
        id: `new-${Math.random().toString(36).slice(2)}`,
        title: "New chat",
        workspace: p.workspace,
        projectId: null,
      }),
    ),
  );
});

afterEach(() => {
  cleanup();
  useStore.setState({ chats: [], projects: [], activeProjectId: null, activeChatId: null, view: "home" });
});

describe("TopBar", () => {
  it("renders the three navigation buttons in the required order", () => {
    seedStore([]);
    const { container } = render(<TopBar />);
    const menu = screen.getByTestId("topbar-menu");
    const home = screen.getByTestId("topbar-home");
    const search = screen.getByTestId("topbar-search");
    // Positional check: menu before home before search in the DOM.
    const pos = (a: Element, b: Element) => a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING;
    expect(pos(menu, home)).toBeTruthy();
    expect(pos(home, search)).toBeTruthy();
    expect(container.querySelector('[data-testid="window-controls"]')).toBeTruthy();
  });

  it("home button switches the app to the home view", () => {
    seedStore([]);
    useStore.setState({ view: "chat" });
    render(<TopBar />);
    fireEvent.click(screen.getByTestId("topbar-home"));
    expect(useStore.getState().view).toBe("home");
  });

  it("shows the active project name in the identity button", () => {
    seedStore([]);
    render(<TopBar />);
    expect(screen.getByTestId("topbar-project").textContent).toContain("Alpha");
  });

  describe("main menu", () => {
    it("opens and the File submenu shows functional actions", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-menu"));
      expect(await screen.findByTestId("main-menu")).toBeTruthy();
      // Submenu opens on click and lists only functional actions.
      fireEvent.click(screen.getByTestId("menu-file"));
      expect(screen.getByTestId("menu-file-sub")).toBeTruthy();
      expect(screen.getByTestId("menu-file-new")).toBeTruthy();
      expect(screen.getByTestId("menu-file-open")).toBeTruthy();
      expect(screen.getByTestId("menu-file-settings")).toBeTruthy();
      expect(screen.getByTestId("menu-file-exit")).toBeTruthy();
    });

    it("New Session in the File submenu creates a session and closes the menu", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-menu"));
      fireEvent.click(screen.getByTestId("menu-file"));
      fireEvent.click(screen.getByTestId("menu-file-new"));
      await waitFor(() => expect(mocks.createChat).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(useStore.getState().view).toBe("chat"));
      // The menu exits with a short animation — wait for it to unmount.
      await waitFor(() => expect(screen.queryByTestId("main-menu")).toBeNull());
    });

    it("keyboard: ArrowRight opens the submenu, Escape closes submenu then menu", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-menu"));
      const menu = await screen.findByTestId("main-menu");
      fireEvent.keyDown(menu, { key: "ArrowRight" });
      expect(screen.getByTestId("menu-file-sub")).toBeTruthy();
      // Arrow keys move within the submenu, Escape returns to the menu first.
      fireEvent.keyDown(menu, { key: "ArrowDown" });
      fireEvent.keyDown(menu, { key: "Escape" });
      expect(screen.queryByTestId("menu-file-sub")).toBeNull();
      expect(screen.getByTestId("main-menu")).toBeTruthy();
      fireEvent.keyDown(menu, { key: "Escape" });
      await waitFor(() => expect(screen.queryByTestId("main-menu")).toBeNull());
    });

    it("Exit closes the window via the Tauri window API", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-menu"));
      fireEvent.click(screen.getByTestId("menu-file"));
      fireEvent.click(screen.getByTestId("menu-file-exit"));
      await waitFor(() => expect(mocks.win.close).toHaveBeenCalledTimes(1));
    });
  });

  describe("search popup", () => {
    const chats = [
      chat({ id: "recent", title: "Fix login bug", projectId: "p1", updatedAt: Date.now() - 1000 }),
      chat({ id: "older", title: "Write README", projectId: "p2", updatedAt: Date.now() - 200_000 }),
    ];

    it("lists recent sessions newest-first and opens one on Enter", async () => {
      seedStore(chats);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-search"));
      const panel = await screen.findByTestId("search-sessions");
      expect(screen.getByText("Recent Sessions")).toBeTruthy();
      const results = panel.querySelectorAll('[data-testid^="search-result-"]');
      expect(results).toHaveLength(2);
      expect(results[0].textContent).toContain("Fix login bug");
      fireEvent.keyDown(screen.getByTestId("search-input"), { key: "Enter" });
      await waitFor(() => expect(mocks.getMessages).toHaveBeenCalledWith("recent"));
      // Opening a session switches to the chat view.
      await waitFor(() => expect(useStore.getState().view).toBe("chat"));
    });

    it("filters results as the user types (debounced)", async () => {
      seedStore(chats);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-search"));
      await screen.findByTestId("search-sessions");
      fireEvent.change(screen.getByTestId("search-input"), { target: { value: "README" } });
      await waitFor(
        () => {
          const panel = screen.getByTestId("search-sessions");
          expect(panel.querySelectorAll('[data-testid^="search-result-"]')).toHaveLength(1);
        },
        { timeout: 1500 },
      );
      // Matches by project name too.
      fireEvent.change(screen.getByTestId("search-input"), { target: { value: "alpha" } });
      await waitFor(
        () => {
          const panel = screen.getByTestId("search-sessions");
          const rows = panel.querySelectorAll('[data-testid^="search-result-"]');
          expect(rows).toHaveLength(1);
          expect(rows[0].textContent).toContain("Fix login bug");
        },
        { timeout: 1500 },
      );
    });

    it("closes on Escape", async () => {
      seedStore(chats);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-search"));
      await screen.findByTestId("search-sessions");
      fireEvent.keyDown(screen.getByTestId("search-input"), { key: "Escape" });
      await waitFor(() => expect(screen.queryByTestId("search-sessions")).toBeNull());
    });
  });

  describe("project switcher", () => {
    it("lists projects, marks the current one and switches on click", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-project"));
      const panel = await screen.findByTestId("project-switcher");
      expect(panel.querySelectorAll('[data-testid^="switcher-project-"]')).toHaveLength(2);
      expect(screen.getByTestId("switcher-current")).toBeTruthy();
      fireEvent.click(screen.getByTestId("switcher-project-1"));
      expect(useStore.getState().activeProjectId).toBe("p2");
      await waitFor(() => expect(screen.queryByTestId("project-switcher")).toBeNull());
    });

    it("Add Project uses the native folder picker flow", async () => {
      seedStore([]);
      mocks.pickAndAddProject.mockResolvedValue(null);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("topbar-project"));
      await screen.findByTestId("project-switcher");
      fireEvent.click(screen.getByTestId("switcher-add"));
      await waitFor(() => expect(mocks.pickAndAddProject).toHaveBeenCalledTimes(1));
    });
  });

  describe("window controls", () => {
    it("minimize, maximize and close use the window API", async () => {
      seedStore([]);
      render(<TopBar />);
      fireEvent.click(screen.getByTestId("win-minimize"));
      fireEvent.click(screen.getByTestId("win-maximize"));
      fireEvent.click(screen.getByTestId("win-close"));
      await waitFor(() => expect(mocks.win.minimize).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(mocks.win.toggleMaximize).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(mocks.win.close).toHaveBeenCalledTimes(1));
    });
  });
});
