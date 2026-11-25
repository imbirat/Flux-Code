import { create } from "zustand";
import { api, mimeForPath } from "./tauri";
import type {
  Attachment,
  ChatCaps,
  ChatMessage,
  ChatSummary,
  MessageMeta,
  PermissionRequest,
  ProcessInfo,
  ProjectInfo,
  ProviderInfo,
  QuestionRequest,
  Settings,
  TodoItem,
  ToolLive,
} from "./types";
import { makeT, type TFn } from "./i18n";

interface StreamingBuf {
  messageId: string;
  text: string;
  thinking: string;
}

export interface ComposerDraft {
  text: string;
  attachments: Attachment[];
}

const EMPTY_DRAFT: ComposerDraft = { text: "", attachments: [] };

const errorText = (e: unknown): string =>
  typeof e === "string" ? e : e instanceof Error ? e.message : String(e);

interface Store {
  booted: boolean;
  settings: Settings;
  providers: ProviderInfo[];
  chats: ChatSummary[];
  activeChatId: string | null;
  projects: ProjectInfo[];
  activeProjectId: string | null;
  /** Top-level navigation: home = Projects/Sessions page, chat = session view. */
  view: "home" | "chat";
  messages: Record<string, ChatMessage[]>;
  /** Unsent text and attachments, isolated per chat and kept across switches. */
  drafts: Record<string, ComposerDraft | undefined>;
  streaming: Record<string, StreamingBuf | undefined>;
  status: Record<string, string | undefined>;
  tools: Record<string, Record<string, ToolLive>>;
  todos: Record<string, TodoItem[]>;
  permission: PermissionRequest | null;
  question: QuestionRequest | null;
  processes: ProcessInfo[];
  servers: { processId: number; url: string }[];
  terminalTail: Record<string, string>;
  errors: Record<string, string | undefined>;
  caps: Record<string, ChatCaps>;
  rightTab: "preview" | "browser" | "todo" | "files";
  rightOpen: boolean;
  leftOpen: boolean;
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  /** Global transient notice (e.g. project add failures). */
  toast: string | null;
  setToast: (msg: string | null) => void;

  t: TFn;
  activeChat: () => ChatSummary | undefined;
  activeProject: () => ProjectInfo | undefined;
  setView: (view: "home" | "chat") => void;
  init: () => Promise<void>;
  applySettings: (patch: Partial<Settings>) => Promise<void>;
  refreshProviders: () => Promise<void>;
  refreshChats: () => Promise<void>;
  refreshProjects: () => Promise<void>;
  setActiveProject: (id: string | null) => void;
  addProject: (path: string, name?: string) => Promise<ProjectInfo | null>;
  renameProject: (id: string, name: string) => Promise<void>;
  removeProject: (id: string) => Promise<void>;
  refreshProcesses: () => Promise<void>;
  loadMessages: (chatId: string) => Promise<void>;
  loadTodos: (chatId: string) => Promise<void>;
  openChat: (id: string) => Promise<void>;
  startNewChat: () => Promise<void>;
  renameChat: (id: string, title: string) => Promise<void>;
  deleteChat: (id: string) => Promise<void>;
  selectModel: (provider: string, model: string) => Promise<void>;
  setEffort: (effort: string) => Promise<void>;
  setPermissionMode: (mode: string) => Promise<void>;
  ensureCaps: (provider: string, model: string) => Promise<ChatCaps | null>;
  setWorkspace: (ws: string) => Promise<void>;
  addExtraFolder: (folder: string) => Promise<void>;
  setDraft: (chatId: string, patch: Partial<ComposerDraft>) => void;
  clearDraft: (chatId: string) => void;
  /** Returns true only when the message was persisted and can be cleared. */
  send: (text: string, attachments: Attachment[]) => Promise<boolean>;
  stop: () => Promise<void>;
  setTodos: (chatId: string, items: TodoItem[]) => Promise<void>;
  respondPermission: (allow: boolean, remember: boolean) => Promise<void>;
  respondQuestion: (answers: Record<string, string | string[]>) => Promise<void>;
  dismissError: (chatId: string) => void;
  setRightTab: (tab: Store["rightTab"]) => void;
  setRightOpen: (open: boolean) => void;
  setLeftOpen: (open: boolean) => void;
  onEvent: (name: string, payload: any) => void;
}

const DEFAULT_SETTINGS: Settings = {
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

export const useStore = create<Store>((set, get) => ({
  booted: false,
  settings: DEFAULT_SETTINGS,
  providers: [],
  chats: [],
  activeChatId: null,
  projects: [],
  activeProjectId: null,
  view: "home",
  messages: {},
  drafts: {},
  streaming: {},
  status: {},
  tools: {},
  todos: {},
  permission: null,
  question: null,
  processes: [],
  servers: [],
  terminalTail: {},
  errors: {},
  caps: {},
  rightTab: "preview",
  rightOpen: true,
  leftOpen: true,
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  toast: null,
  setToast: (msg) => set({ toast: msg }),

  t: makeT("default"),
  activeChat: () => {
    const { chats, activeChatId } = get();
    return chats.find((c) => c.id === activeChatId);
  },

  activeProject: () => {
    const { projects, activeProjectId } = get();
    return projects.find((p) => p.id === activeProjectId);
  },

  setView: (view) => set({ view }),

  init: async () => {
    try {
      const settings = { ...DEFAULT_SETTINGS, ...(await api.getSettings()) };
      set({ settings, t: makeT(settings.language) });
    } catch {
      /* keep defaults */
    }
    await get().refreshProjects();
    await get().refreshProviders();
    await get().refreshChats();
    await get().refreshProcesses().catch(() => {});
    try {
      const ws = await api.getDefaultWorkspace();
      if (!get().settings.workspace) {
        set((s) => ({ settings: { ...s.settings, workspace: ws } }));
      }
    } catch {
      /* ignore */
    }
    // Restore the last used project. An empty `lastProjectId` means the user
    // deliberately chose All Projects, so do not silently select a project.
    const { settings, projects } = get();
    const last = settings.lastProjectId;
    if (last === undefined) {
      if (projects.length > 0) set({ activeProjectId: projects[0].id });
    } else if (last && projects.some((p) => p.id === last)) {
      set({ activeProjectId: last });
    } else {
      set({ activeProjectId: null });
    }
    set({ booted: true });
  },

  applySettings: async (patch) => {
    set((s) => ({ settings: { ...s.settings, ...patch }, t: makeT(patch.language ?? s.settings.language) }));
    try {
      const merged = await api.setSettings(patch);
      set((s) => ({ settings: { ...s.settings, ...merged } }));
    } catch {
      /* ignore */
    }
  },

  refreshProviders: async () => {
    try {
      const providers = await api.providerStatus();
      set({ providers });
    } catch {
      /* ignore */
    }
  },

  refreshChats: async () => {
    try {
      const chats = await api.listChats();
      set({ chats });
    } catch {
      /* ignore */
    }
  },

  refreshProjects: async () => {
    try {
      const projects = await api.projectList();
      set((s) => ({
        projects,
        // A null selection is the user's explicit "All Projects" choice and
        // must survive refreshes; only a stale id falls back to it.
        activeProjectId:
          s.activeProjectId && projects.some((p) => p.id === s.activeProjectId)
            ? s.activeProjectId
            : null,
      }));
    } catch {
      /* ignore */
    }
  },

  setActiveProject: (id) => {
    set({ activeProjectId: id });
    // Persist the choice so it survives restarts (best effort). An empty
    // string records the explicit "All Projects" selection.
    get().applySettings({ lastProjectId: id ?? "" }).catch(() => {});
  },

  addProject: async (path, name) => {
    try {
      const project = await api.projectAdd(path, name);
      await get().refreshProjects();
      set({ activeProjectId: project.id });
      get().applySettings({ lastProjectId: project.id }).catch(() => {});
      return project;
    } catch (e) {
      throw new Error(typeof e === "string" ? e : String(e));
    }
  },

  renameProject: async (id, name) => {
    try {
      const updated = await api.projectRename(id, name);
      set((s) => ({ projects: s.projects.map((p) => (p.id === id ? updated : p)) }));
    } catch (e) {
      throw new Error(typeof e === "string" ? e : String(e));
    }
  },

  removeProject: async (id) => {
    try {
      await api.projectRemove(id);
      await get().refreshChats();
      await get().refreshProjects();
      // If the active chat belonged to the removed project it is now unlinked;
      // session data stays intact. Fall back to All Projects rather than
      // silently selecting a different project.
      if (get().activeProjectId === id) {
        set({ activeProjectId: null });
      }
    } catch (e) {
      throw new Error(typeof e === "string" ? e : String(e));
    }
  },

  refreshProcesses: async () => {
    try {
      const processes = await api.listProcesses();
      set({ processes });
    } catch {
      /* ignore */
    }
  },

  loadMessages: async (chatId) => {
    try {
      const messages = await api.getMessages(chatId);
      set((s) => ({ messages: { ...s.messages, [chatId]: messages } }));
    } catch {
      /* ignore */
    }
  },

  loadTodos: async (chatId) => {
    try {
      const todos = await api.getTodos(chatId);
      set((s) => ({ todos: { ...s.todos, [chatId]: todos } }));
    } catch {
      /* ignore */
    }
  },

  openChat: async (id) => {
    set({ activeChatId: id, errors: { ...get().errors, [id]: undefined } });
    await Promise.all([get().loadMessages(id), get().loadTodos(id)]);
    const chat = get().chats.find((c) => c.id === id);
    if (chat?.provider && chat?.model) {
      get().ensureCaps(chat.provider, chat.model).catch(() => {});
    }
    // Switch to the chat's project so the top bar identity stays truthful.
    if (chat?.projectId) {
      set({ view: "chat", activeProjectId: chat.projectId });
    } else {
      set({ view: "chat" });
    }
  },

  startNewChat: async () => {
    const s = get();
    const project = s.activeProject();
    // Debounce double clicks: reuse a just-created empty chat of this project
    // instead of stacking blank sessions.
    const recentEmpty = s.chats.find(
      (c) =>
        (!project || c.projectId === project.id) &&
        Date.now() - c.updatedAt < 2000 &&
        !(s.messages[c.id]?.length ?? 0),
    );
    if (recentEmpty) {
      await get().openChat(recentEmpty.id);
      return;
    }
    const ws = project?.path || s.settings.workspace || (await api.getDefaultWorkspace().catch(() => ""));
    const chat = await api.createChat({
      workspace: ws,
      provider: s.settings.lastProvider,
      model: s.settings.lastModel,
      effort: "",
      permissionMode: s.settings.defaultPermissionMode,
      projectId: project?.id ?? null,
    });
    set((st) => ({
      chats: [chat, ...st.chats],
      activeChatId: chat.id,
      view: "chat",
      activeProjectId: chat.projectId ?? st.activeProjectId,
      messages: { ...st.messages, [chat.id]: [] },
      todos: { ...st.todos, [chat.id]: [] },
      tools: { ...st.tools, [chat.id]: {} },
      status: { ...st.status, [chat.id]: undefined },
      streaming: { ...st.streaming, [chat.id]: undefined },
    }));
  },

  renameChat: async (id, title) => {
    const previous = get().chats.find((c) => c.id === id);
    if (!previous) return;
    set((s) => ({ chats: s.chats.map((c) => (c.id === id ? { ...c, title } : c)) }));
    try {
      await api.renameChat(id, title);
    } catch (e) {
      // Roll back only if this request is still the visible title.
      set((s) => ({
        chats: s.chats.map((c) => (c.id === id && c.title === title ? previous : c)),
        toast: errorText(e),
      }));
    }
  },

  deleteChat: async (id) => {
    try {
      await api.deleteChat(id);
    } catch (e) {
      // The row (and its files) still exists; keep it selected and report.
      set({ toast: errorText(e) });
      await get().refreshChats();
      return;
    }
    await get().refreshChats();
    set((s) => {
      const drafts = { ...s.drafts };
      delete drafts[id];
      return { drafts };
    });
    if (get().activeChatId === id) {
      // Never auto-open another session — return to the home page.
      set({ activeChatId: null, view: "home", messages: { ...get().messages, [id]: [] } });
    }
  },

  ensureCaps: async (provider, model) => {
    if (!provider || !model) return null;
    const key = `${provider}|${model}`;
    const cached = get().caps[key];
    if (cached) return cached;
    try {
      const caps = await api.modelCapabilities(provider, model);
      set((s) => ({ caps: { ...s.caps, [key]: caps } }));
      return caps;
    } catch {
      return null;
    }
  },

  selectModel: async (provider, model) => {
    const id = get().activeChatId;
    const previousChat = id ? get().chats.find((c) => c.id === id) : undefined;
    const previousProvider = get().settings.lastProvider;
    const previousModel = get().settings.lastModel;
    await get().ensureCaps(provider, model);
    await get().applySettings({ lastProvider: provider, lastModel: model });
    if (!id) return;
    // Optimistic switch so the composer reflects the choice immediately.
    set((s) => ({
      chats: s.chats.map((c) => (c.id === id ? { ...c, provider, model, effort: "" } : c)),
    }));
    try {
      const updated = await api.updateChat(id, { provider, model, effort: "" });
      set((s) => ({ chats: s.chats.map((c) => (c.id === id ? updated : c)) }));
    } catch (e) {
      set((s) => ({
        chats: s.chats.map((c) =>
          c.id === id && c.provider === provider && c.model === model && previousChat
            ? previousChat
            : c,
        ),
        toast: errorText(e),
      }));
      get()
        .applySettings({ lastProvider: previousProvider, lastModel: previousModel })
        .catch(() => {});
    }
  },

  setEffort: async (effort) => {
    const s = get();
    const chat = s.activeChat();
    if (!chat) return;
    set((st) => ({
      chats: st.chats.map((c) => (c.id === chat.id ? { ...c, effort } : c)),
    }));
    try {
      const updated = await api.updateChat(chat.id, { effort });
      set((st) => ({ chats: st.chats.map((c) => (c.id === chat.id ? updated : c)) }));
      const key = `${chat.provider}|${chat.model}`;
      await get().applySettings({
        effortPrefs: { ...s.settings.effortPrefs, [key]: effort },
      });
    } catch (e) {
      set((st) => ({
        chats: st.chats.map((c) =>
          c.id === chat.id && c.effort === effort ? { ...c, effort: chat.effort } : c,
        ),
        toast: errorText(e),
      }));
    }
  },

  setPermissionMode: async (mode) => {
    const chat = get().activeChat();
    if (!chat) return;
    set((st) => ({
      chats: st.chats.map((c) => (c.id === chat.id ? { ...c, permissionMode: mode } : c)),
    }));
    try {
      const updated = await api.updateChat(chat.id, { permissionMode: mode });
      set((st) => ({ chats: st.chats.map((c) => (c.id === chat.id ? updated : c)) }));
    } catch (e) {
      set((st) => ({
        chats: st.chats.map((c) =>
          c.id === chat.id && c.permissionMode === mode
            ? { ...c, permissionMode: chat.permissionMode }
            : c,
        ),
        toast: errorText(e),
      }));
    }
  },

  setWorkspace: async (ws) => {
    const previous = get().settings.workspace;
    await get().applySettings({ workspace: ws });
    const chat = get().activeChat();
    if (!chat) return;
    try {
      const updated = await api.updateChat(chat.id, { workspace: ws });
      set((st) => ({ chats: st.chats.map((c) => (c.id === chat.id ? updated : c)) }));
    } catch (e) {
      // Restore the default workspace too: the session keeps its old folder.
      await get().applySettings({ workspace: previous });
      set({ toast: errorText(e) });
    }
  },

  addExtraFolder: async (folder) => {
    const folders = [...new Set([...get().settings.extraFolders, folder])];
    await get().applySettings({ extraFolders: folders });
  },

  setDraft: (chatId, patch) =>
    set((s) => {
      const prev = s.drafts[chatId] ?? EMPTY_DRAFT;
      return { drafts: { ...s.drafts, [chatId]: { ...prev, ...patch } } };
    }),

  clearDraft: (chatId) =>
    set((s) => {
      const drafts = { ...s.drafts };
      delete drafts[chatId];
      return { drafts };
    }),

  send: async (text, attachments) => {
    let chatId = get().activeChatId;
    if (!chatId) {
      try {
        await get().startNewChat();
      } catch (e) {
        set({ toast: errorText(e) });
        return false;
      }
      chatId = get().activeChatId;
    }
    if (!chatId) return false;
    const draft = get().drafts[chatId];
    try {
      await api.sendMessage(chatId, text, attachments);
      // Consume the draft only after persistence, and only if the user has not
      // typed or attached something newer while the request was in flight.
      if (draft) {
        set((st) => {
          const current = st.drafts[chatId];
          if (
            !current ||
            current.text !== draft.text ||
            current.attachments !== draft.attachments
          ) {
            return {};
          }
          const drafts = { ...st.drafts };
          delete drafts[chatId];
          return { drafts };
        });
      }
      await Promise.all([get().loadMessages(chatId), get().refreshChats()]);
      return true;
    } catch (e) {
      set((st) => ({ errors: { ...st.errors, [chatId]: errorText(e) } }));
      return false;
    }
  },

  stop: async () => {
    const id = get().activeChatId;
    if (!id) return;
    try {
      await api.cancelChat(id);
    } catch {
      /* ignore */
    }
    set((st) => ({
      streaming: { ...st.streaming, [id]: undefined },
      status: { ...st.status, [id]: undefined },
    }));
  },

  setTodos: async (chatId, items) => {
    const normalized = items.map((t, i) => ({ ...t, sort: i, status: t.status || "pending" }));
    set((s) => ({ todos: { ...s.todos, [chatId]: normalized } }));
    try {
      await api.setTodos(chatId, normalized);
    } catch {
      /* ignore */
    }
  },

  respondPermission: async (allow, remember) => {
    const p = get().permission;
    if (!p) return;
    set({ permission: null });
    try {
      await api.respondPermission(p.id, allow, remember);
    } catch {
      /* ignore */
    }
  },

  respondQuestion: async (answers) => {
    const q = get().question;
    if (!q) return;
    set({ question: null });
    try {
      await api.respondQuestions(q.id, answers);
    } catch {
      /* ignore */
    }
  },

  dismissError: (chatId) => set((s) => ({ errors: { ...s.errors, [chatId]: undefined } })),

  setRightTab: (tab) => set({ rightTab: tab, rightOpen: true }),
  setRightOpen: (open) => set({ rightOpen: open }),
  setLeftOpen: (open) => set({ leftOpen: open }),

  onEvent: async (name, p) => {
    const s = get();
    switch (name) {
      case "chat:start": {
        set((st) => ({
          streaming: { ...st.streaming, [p.chatId]: { messageId: p.messageId, text: "", thinking: "" } },
          status: { ...st.status, [p.chatId]: st.status[p.chatId] ?? "thinking" },
          tools: { ...st.tools, [p.chatId]: {} },
        }));
        break;
      }
      case "chat:delta": {
        const buf = s.streaming[p.chatId];
        if (!buf || buf.messageId !== p.messageId) break;
        const next = { ...buf };
        if (p.kind === "text") next.text += p.value;
        else if (p.kind === "thinking") next.thinking += p.value;
        set((st) => ({ streaming: { ...st.streaming, [p.chatId]: next } }));
        break;
      }
      case "chat:message": {
        set((st) => {
          const arr = st.messages[p.chatId] ?? [];
          const exists = arr.some((m) => m.id === p.message.id);
          const messages = exists
            ? { ...st.messages, [p.chatId]: arr.map((m) => (m.id === p.message.id ? p.message : m)) }
            : { ...st.messages, [p.chatId]: [...arr, p.message] };
          const buf = st.streaming[p.chatId];
          const streaming =
            buf && buf.messageId === p.message.id
              ? { ...st.streaming, [p.chatId]: undefined }
              : st.streaming;
          return { messages, streaming };
        });
        break;
      }
      case "chat:tool": {
        set((st) => {
          const forChat = { ...(st.tools[p.chatId] ?? {}) };
          forChat[p.callId] = {
            state: p.state,
            name: p.name,
            args: p.args,
            result: p.result,
            meta: (p.meta ?? undefined) as MessageMeta | undefined,
          };
          return { tools: { ...st.tools, [p.chatId]: forChat } };
        });
        break;
      }
      case "chat:status": {
        set((st) => ({ status: { ...st.status, [p.chatId]: p.status } }));
        break;
      }
      case "chat:done": {
        set((st) => ({
          streaming: { ...st.streaming, [p.chatId]: undefined },
          status: { ...st.status, [p.chatId]: undefined },
        }));
        get().refreshChats();
        break;
      }
      case "chat:error": {
        set((st) => ({
          errors: { ...st.errors, [p.chatId]: p.error },
          streaming: { ...st.streaming, [p.chatId]: undefined },
          status: { ...st.status, [p.chatId]: undefined },
        }));
        break;
      }
      case "chat:title": {
        set((st) => ({
          chats: st.chats.map((c) => (c.id === p.chatId ? { ...c, title: p.title } : c)),
        }));
        break;
      }
      case "todo:update": {
        set((st) => ({ todos: { ...st.todos, [p.chatId]: p.todos } }));
        break;
      }
      case "permission:request": {
        set({ permission: p as PermissionRequest });
        // The agent is blocked waiting for input — bring the session to the
        // front so the permission card is reachable.
        const req = p as PermissionRequest;
        if (req.chatId && req.chatId !== get().activeChatId) {
          await get().openChat(req.chatId).catch(() => {});
        } else {
          set({ view: "chat" });
        }
        break;
      }
      case "question:request": {
        set({ question: p as QuestionRequest });
        const req = p as QuestionRequest;
        if (req.chatId && req.chatId !== get().activeChatId) {
          await get().openChat(req.chatId).catch(() => {});
        } else {
          set({ view: "chat" });
        }
        break;
      }
      case "terminal:chunk": {
        const prev = s.terminalTail[p.chatId] ?? "";
        set((st) => ({ terminalTail: { ...st.terminalTail, [p.chatId]: (prev + p.data).slice(-500) } }));
        break;
      }
      case "preview:server": {
        const exists = s.servers.some((x) => x.processId === p.processId && x.url === p.url);
        if (!exists) set((st) => ({ servers: [...st.servers, { processId: p.processId, url: p.url }] }));
        break;
      }
      case "process:exit": {
        get().refreshProcesses().catch(() => {});
        break;
      }
      default:
        break;
    }
  },
}));

export const imageMime = mimeForPath;
