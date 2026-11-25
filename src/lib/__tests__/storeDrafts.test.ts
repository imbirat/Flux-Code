import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatSummary } from "../types";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn(),
  getMessages: vi.fn(),
  listChats: vi.fn(),
  updateChat: vi.fn(),
  renameChat: vi.fn(),
  deleteChat: vi.fn(),
  setSettings: vi.fn(),
  modelCapabilities: vi.fn(),
  getTodos: vi.fn(),
}));

vi.mock("../tauri", () => ({
  api: mocks,
  mimeForPath: () => "image/png",
}));

import { useStore } from "../store";

const chat: ChatSummary = {
  id: "c1",
  title: "Session",
  workspace: "C:\\work",
  provider: "openai",
  model: "gpt",
  effort: "",
  permissionMode: "ask",
  createdAt: 1,
  updatedAt: 1,
  projectId: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getMessages.mockResolvedValue([]);
  mocks.listChats.mockResolvedValue([chat]);
  mocks.setSettings.mockImplementation(async (patch: Record<string, unknown>) => ({
    ...useStore.getState().settings,
    ...patch,
  }));
  mocks.modelCapabilities.mockResolvedValue({ supports: true, values: ["low"], default: "low" });
  mocks.getTodos.mockResolvedValue([]);

  useStore.setState({
    chats: [chat],
    activeChatId: chat.id,
    messages: {},
    drafts: {},
    errors: {},
    toast: null,
    settings: {
      ...useStore.getState().settings,
      workspace: "C:\\work",
      lastProvider: "openai",
      lastModel: "gpt",
    },
  });
});

describe("composer drafts", () => {
  it("isolates drafts per chat and clears only the requested chat", () => {
    const s = useStore.getState();
    s.setDraft("c1", { text: "first" });
    s.setDraft("c2", { text: "second" });
    expect(useStore.getState().drafts.c1?.text).toBe("first");
    expect(useStore.getState().drafts.c2?.text).toBe("second");

    useStore.getState().clearDraft("c1");
    expect(useStore.getState().drafts.c1).toBeUndefined();
    expect(useStore.getState().drafts.c2?.text).toBe("second");
  });

  it("keeps the draft when sending fails", async () => {
    useStore.getState().setDraft("c1", { text: "do not lose me" });
    mocks.sendMessage.mockRejectedValueOnce("network down");

    const ok = await useStore.getState().send("do not lose me", []);

    expect(ok).toBe(false);
    expect(useStore.getState().drafts.c1?.text).toBe("do not lose me");
    expect(useStore.getState().errors.c1).toContain("network down");
  });

  it("consumes the draft only after a successful send", async () => {
    useStore.getState().setDraft("c1", { text: "send me" });
    mocks.sendMessage.mockResolvedValueOnce(undefined);

    const ok = await useStore.getState().send("send me", []);

    expect(ok).toBe(true);
    expect(useStore.getState().drafts.c1).toBeUndefined();
  });

  it("does not erase newer text typed while the send was in flight", async () => {
    useStore.getState().setDraft("c1", { text: "first" });
    mocks.sendMessage.mockImplementationOnce(async () => {
      useStore.getState().setDraft("c1", { text: "second" });
    });

    const ok = await useStore.getState().send("first", []);

    expect(ok).toBe(true);
    expect(useStore.getState().drafts.c1?.text).toBe("second");
  });
});

describe("store rollback", () => {
  it("restores the old title when rename fails", async () => {
    mocks.renameChat.mockRejectedValueOnce("rename denied");
    await useStore.getState().renameChat("c1", "New title");

    expect(useStore.getState().chats[0].title).toBe("Session");
    expect(useStore.getState().toast).toContain("rename denied");
  });

  it("restores the previous model when the chat update fails", async () => {
    mocks.updateChat.mockRejectedValueOnce("update denied");
    await useStore.getState().selectModel("anthropic", "claude");

    expect(useStore.getState().chats[0].provider).toBe("openai");
    expect(useStore.getState().chats[0].model).toBe("gpt");
    expect(useStore.getState().settings.lastProvider).toBe("openai");
    expect(useStore.getState().settings.lastModel).toBe("gpt");
    expect(useStore.getState().toast).toContain("update denied");
  });

  it("restores permission mode when the chat update fails", async () => {
    mocks.updateChat.mockRejectedValueOnce("update denied");
    await useStore.getState().setPermissionMode("full");

    expect(useStore.getState().chats[0].permissionMode).toBe("ask");
    expect(useStore.getState().toast).toContain("update denied");
  });
});
