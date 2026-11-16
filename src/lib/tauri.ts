import { invoke } from "@tauri-apps/api/core";
import type {
  Attachment,
  ChatCaps,
  ChatMessage,
  ChatSummary,
  ProcessInfo,
  ProviderConfig,
  ProviderInfo,
  ProviderStatus,
  Settings,
  TodoItem,
} from "./types";

export interface TreeNode {
  name: string;
  path: string;
  kind: "dir" | "file";
  depth: number;
}

export const api = {
  // chats + messages
  listChats: () => invoke<ChatSummary[]>("list_chats"),
  createChat: (p: { workspace: string; provider: string; model: string; effort: string; permissionMode: string }) =>
    invoke<ChatSummary>("create_chat", p),
  renameChat: (chatId: string, title: string) => invoke<void>("rename_chat", { chatId, title }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  updateChat: (chatId: string, patch: Record<string, unknown>) =>
    invoke<ChatSummary>("update_chat", { chatId, patch }),
  getMessages: (chatId: string) => invoke<ChatMessage[]>("get_messages", { chatId }),
  sendMessage: (chatId: string, text: string, attachments: Attachment[]) =>
    invoke<void>("send_message", { chatId, text, attachments }),
  cancelChat: (chatId: string) => invoke<void>("cancel_chat", { chatId }),

  // permissions + questions
  respondPermission: (id: string, allow: boolean, remember: boolean) =>
    invoke<void>("respond_permission", { id, allow, remember }),
  respondQuestions: (id: string, answers: Record<string, string | string[]>) =>
    invoke<void>("respond_questions", { id, answers }),

  // todos
  getTodos: (chatId: string) => invoke<TodoItem[]>("get_todos", { chatId }),
  setTodos: (chatId: string, todos: TodoItem[]) => invoke<void>("set_todos", { chatId, todos }),

  // settings
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: Partial<Settings>) => invoke<Settings>("set_settings", { patch }),
  getDefaultWorkspace: () => invoke<string>("get_default_workspace"),

  // providers
  providerStatus: () => invoke<ProviderInfo[]>("provider_status"),
  providerSetKey: (id: string, key: string) => invoke<void>("provider_set_key", { id, key }),
  providerRemoveKey: (id: string) => invoke<void>("provider_remove_key", { id }),
  providerTest: (id: string) =>
    invoke<{
      ok: boolean;
      status: ProviderStatus;
      message: string;
      models?: string[];
    }>("provider_test", { id }),
  providerListModels: (id: string) => invoke<string[]>("provider_list_models", { id }),
  providerGetConfig: (id: string) => invoke<ProviderConfig>("provider_get_config", { id }),
  providerSetConfig: (id: string, config: ProviderConfig) =>
    invoke<void>("provider_set_config", { id, config }),
  customProviderSave: (
    provider: Record<string, unknown>,
    apiKey?: string,
  ) => invoke<ProviderInfo>("custom_provider_save", { provider, apiKey }),
  customProviderDelete: (id: string) => invoke<void>("custom_provider_delete", { id }),
  modelCapabilities: (provider: string, model: string) =>
    invoke<ChatCaps>("model_capabilities", { provider, model }),

  // files + processes
  listDirTree: (path: string, depth: number) => invoke<TreeNode[]>("list_dir_tree", { path, depth }),
  readTextFile: (path: string, maxBytes?: number) =>
    invoke<{ content: string; truncated: boolean; size: number }>("read_text_file", { path, maxBytes }),
  readFileBase64: (path: string) => invoke<string>("read_file_base64", { path }),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
  listProcesses: () => invoke<ProcessInfo[]>("list_processes"),
  stopProcess: (id: number) => invoke<void>("stop_process", { id }),
};

export const mimeForPath = (path: string): string => {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    bmp: "image/bmp",
    svg: "image/svg+xml",
  };
  return map[ext] ?? "image/png";
};
