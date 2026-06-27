import { invoke, Channel } from "@tauri-apps/api/core";

export type AccessMode = "read-only" | "workspace-edit" | "full-access";
export type ReasoningEffort = "off" | "low" | "medium" | "high";

export interface ConversationInfo {
  id: string;
  cwd: string;
  model: string;
  accessMode: AccessMode;
  reasoningEffort: ReasoningEffort;
  path: string;
  recordCount: number;
}

export interface ChatSummary {
  id: string;
  createdAt: string;
  model: string;
  cwd: string;
  firstUserPreview: string;
}

export interface ModelOption {
  id: string;
  provider: string;
  displayName?: string;
  reasoningSupported: boolean;
  reasoningRequired: boolean;
  defaultReasoningEffort: ReasoningEffort;
}

export interface NewSessionArgs {
  cwd?: string;
  accessMode?: AccessMode;
  model?: string;
  baseUrl?: string;
  apiKeyEnv?: string;
  reasoningEffort?: ReasoningEffort;
}

export interface UpdateSessionSettingsArgs {
  chatId: string;
  accessMode?: AccessMode;
  model?: string;
  reasoningEffort?: ReasoningEffort;
}

export interface TurnHandle {
  turnId: string;
  chatId: string;
}

export interface StoredToolCall {
  id: string;
  name: string;
  arguments: unknown;
}

export type ConversationRecord =
  | {
      type: "meta";
      chat_id: string;
      created_at: string;
      model: string;
      cwd: string;
      parent_chat_id?: string | null;
    }
  | { type: "system"; content: string }
  | { type: "user"; content: string; ts: string }
  | {
      type: "assistant";
      content: string;
      reasoning?: string;
      reasoning_field?: string | null;
      tool_calls?: StoredToolCall[];
      ts: string;
    }
  | {
      type: "tool";
      tool_call_id: string;
      name: string;
      ok: boolean;
      content: string;
      ts: string;
    };

export type StreamEvent =
  | { kind: "assistantChunk"; text: string }
  | { kind: "reasoningChunk"; text: string }
  | {
      kind: "toolCallStarted";
      id: string;
      name: string;
      arguments: unknown;
    }
  | {
      kind: "toolOutputChunk";
      id: string;
      name: string;
      stream: string;
      content: string;
    }
  | {
      kind: "toolResult";
      id: string;
      name: string;
      ok: boolean;
      content: string;
    }
  | {
      kind: "approvalRequested";
      requestId: string;
      toolCallId: string;
      name: string;
      arguments: unknown;
      reason: string;
    }
  | { kind: "approvalResolved"; requestId: string; approved: boolean }
  | { kind: "status"; text: string }
  | { kind: "finished" }
  | { kind: "error"; message: string };

export async function newSession(
  args: NewSessionArgs,
): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("new_session", { args });
}

export async function resumeSession(
  chatId: string,
  cwd?: string,
): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("resume_session", { args: { chatId, cwd } });
}

export async function listChats(cwd?: string): Promise<ChatSummary[]> {
  return invoke<ChatSummary[]>("list_chats_cmd", { args: { cwd } });
}

export async function listModels(): Promise<ModelOption[]> {
  return invoke<ModelOption[]>("list_models_cmd");
}

export async function updateSessionSettings(
  args: UpdateSessionSettingsArgs,
): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("update_session_settings", { args });
}

export async function getCwd(): Promise<string> {
  return invoke<string>("get_cwd");
}

export async function sessionInfo(chatId: string): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("session_info", { args: { chatId } });
}

export async function sessionRecords(
  chatId: string,
): Promise<ConversationRecord[]> {
  return invoke<ConversationRecord[]>("session_records", { args: { chatId } });
}

export async function startTurn(
  chatId: string,
  message: string,
  onEvent: (event: StreamEvent) => void,
): Promise<TurnHandle> {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<TurnHandle>("start_turn", { chatId, message, onEvent: channel });
}

export async function approve(turnId: string, requestId: string): Promise<void> {
  return invoke("approve", { args: { turnId, requestId } });
}

export async function deny(turnId: string, requestId: string): Promise<void> {
  return invoke("deny", { args: { turnId, requestId } });
}

export async function cancelTurn(
  turnId: string,
): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("cancel_turn", { args: { turnId } });
}
