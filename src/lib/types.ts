export type Role = "user" | "assistant" | "tool";

export interface ToolCallInfo {
  id: string;
  name: string;
  args: string;
}

export interface DiffRow {
  type: "add" | "del" | "same";
  text: string;
  oldNo?: number;
  newNo?: number;
}

export interface QuestionDef {
  id: string;
  type: "choice" | "multi" | "text" | "yesno";
  question: string;
  options?: string[];
  optional?: boolean;
}

export interface MessageMeta {
  toolCalls?: ToolCallInfo[];
  toolCallId?: string;
  toolName?: string;
  state?: string;
  thinking?: string;
  rawBlocks?: unknown;
  images?: { name: string; mime: string; data: string }[];
  attachments?: Attachment[];
  card?: { questions: QuestionDef[]; answers?: Record<string, string | string[]> };
  diff?: DiffRow[];
  command?: string;
  cwd?: string;
  exitCode?: number | null;
  timedOut?: boolean;
  output?: string;
  processId?: number;
  path?: string;
  newPath?: string;
  bytes?: number;
  truncated?: boolean;
  [key: string]: unknown;
}

export interface ChatMessage {
  id: string;
  chatId: string;
  role: Role;
  content: string;
  meta: MessageMeta;
  createdAt: number;
}

export interface TodoItem {
  id: string;
  content: string;
  status: "pending" | "in_progress" | "completed";
  sort: number;
}

export interface ChatSummary {
  id: string;
  title: string;
  workspace: string;
  provider: string;
  model: string;
  effort: string;
  permissionMode: string;
  createdAt: number;
  updatedAt: number;
}

export interface Attachment {
  kind: "file" | "folder" | "image";
  path: string;
  name: string;
  mime?: string;
  data?: string;
}

export type ProviderStatus =
  | "not_configured"
  | "untested"
  | "testing"
  | "connected"
  | "failed";

export interface ProviderConfig {
  enabledModels?: string[];
  defaultModel?: string | null;
  baseUrl?: string;
  format?: string;
  headers?: Record<string, string>;
  modelsPath?: string;
  [key: string]: unknown;
}

export interface ProviderInfo {
  id: string;
  label: string;
  format: string;
  baseUrl: string;
  helpUrl: string;
  custom: boolean;
  status: ProviderStatus;
  message: string;
  config: ProviderConfig;
}

export interface Settings {
  theme: "system" | "dark" | "light";
  accent: "flux" | "blue" | "purple" | "red" | "white";
  language: string;
  reducedMotion: boolean;
  workspace: string;
  extraFolders: string[];
  defaultPermissionMode: string;
  effortPrefs: Record<string, string>;
  lastProvider: string;
  lastModel: string;
}

export interface PermissionRequest {
  id: string;
  chatId: string;
  tool: string;
  summary: string;
  detail: { kind: string; command?: string; cwd?: string; path?: string; newPath?: string };
}

export interface QuestionRequest {
  id: string;
  chatId: string;
  questions: QuestionDef[];
}

export interface ToolLive {
  state: string;
  name?: string;
  args?: Record<string, unknown>;
  result?: string;
  meta?: MessageMeta;
}

export interface ProcessInfo {
  id: number;
  name: string;
  command: string;
  startedAt: number;
  exited: number;
  recentOutput: string[];
}

export interface ChatCaps {
  supports: boolean;
  values: string[];
  default: string;
}

export interface TreeNode {
  name: string;
  path: string;
  kind: "dir" | "file";
  depth: number;
}
