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

export async function reloadSessionConfig(
  chatId: string,
): Promise<ConversationInfo> {
  return invoke<ConversationInfo>("reload_session_config", { args: { chatId } });
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

// --- Slash commands ---------------------------------------------------------

export interface CommandSpec {
  name: string;
  usage: string;
  description: string;
  takesValue: boolean;
}

export interface ProviderCatalogEntry {
  name: string;
  id: string;
  baseUrl: string;
  apiKeyEnv: string;
}

export interface AutoFillItem {
  label: string;
  insert: string;
  detail?: string;
}

export interface AutoFillMenu {
  title: string;
  replacementStart: number;
  replacementEnd: number;
  items: AutoFillItem[];
  selected: number;
}

export type CheckpointKind = "user" | "assistant" | "tool_call" | "tool_result";

export interface BranchSummary {
  id: string;
  createdAt: string;
  parentChatId?: string | null;
  branchLabel?: string | null;
  recordCount: number;
  current: boolean;
}

export interface Checkpoint {
  id: string;
  chatId: string;
  recordIndex: number;
  toolCallId?: string | null;
  kind: CheckpointKind;
  label: string;
  detail: string;
  ts?: string | null;
}

export interface BranchFamily {
  branches: BranchSummary[];
  checkpoints: Checkpoint[];
}

export interface ProviderLogoutCandidate {
  id: string;
  name?: string | null;
  defaultModel?: string | null;
  modelCount: number;
}

export interface LogoutResult {
  removedProviderIds: string[];
  removedModelCount: number;
  remainingProviderCount: number;
  activeProvider?: string | null;
  activeModel?: string | null;
}

export interface RestoreReport {
  summary: string;
  applied: number;
  skipped: number;
  conflicts: number;
}

export interface BranchResult {
  info: ConversationInfo;
  sourceChatId: string;
  status: string;
  restore?: RestoreReport | null;
}

export interface SetupSelection {
  providerId: string;
  providerName: string;
  baseUrl: string;
  apiKeyEnv: string;
  modelId: string;
  supportsTools: boolean;
  supportsReasoning: boolean;
}

export interface ProviderApplyResult {
  activeProvider: string;
  activeModel: string;
}

export type CommandOutcome =
  | { kind: "status"; title: string; content: string }
  | { kind: "newChat"; info: ConversationInfo; status: string }
  | {
      kind: "resumedChat";
      info: ConversationInfo;
      warning?: string | null;
      status: string;
    }
  | { kind: "openBranchPicker"; family: BranchFamily }
  | { kind: "openLoginWizard" }
  | { kind: "openLogoutPicker"; candidates: ProviderLogoutCandidate[] }
  | { kind: "busy"; message: string }
  | { kind: "parseError"; message: string }
  | { kind: "error"; title: string; message: string };

export async function listSlashCommands(): Promise<CommandSpec[]> {
  return invoke<CommandSpec[]>("list_slash_commands");
}

export async function listProviderCatalog(): Promise<ProviderCatalogEntry[]> {
  return invoke<ProviderCatalogEntry[]>("list_provider_catalog");
}

export async function slashAutofill(
  input: string,
  cwd?: string,
): Promise<AutoFillMenu | null> {
  return invoke<AutoFillMenu | null>("slash_autofill", { args: { input, cwd } });
}

export async function runSlashCommand(
  chatId: string,
  input: string,
): Promise<CommandOutcome> {
  return invoke<CommandOutcome>("run_slash_command", { args: { chatId, input } });
}

export async function createBranchFromCheckpoint(
  chatId: string,
  checkpoint: Checkpoint,
  restoreFiles: boolean,
): Promise<BranchResult> {
  return invoke<BranchResult>("create_branch_from_checkpoint", {
    args: { chatId, checkpoint, restoreFiles },
  });
}

export async function applyProviderLogin(
  selections: SetupSelection[],
  activeIndex: number,
): Promise<ProviderApplyResult> {
  return invoke<ProviderApplyResult>("apply_provider_login", {
    args: { selections, activeIndex },
  });
}

export async function discoverModels(
  baseUrl: string,
  apiKey: string,
): Promise<string[]> {
  return invoke<string[]>("discover_models", { args: { baseUrl, apiKey } });
}

export async function removeProviders(
  providerIds: string[],
): Promise<LogoutResult> {
  return invoke<LogoutResult>("remove_providers", { args: { providerIds } });
}
