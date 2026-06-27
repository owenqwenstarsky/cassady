use cassady::access::AccessMode;
use cassady::branch::{BranchFamily, BranchSummary, Checkpoint, CheckpointKind};
use cassady::commands::CommandSpec;
use cassady::config::{ModelDefinition, ReasoningEffort};
use cassady::setup::{LogoutResult, ProviderCatalogEntry, ProviderLogoutCandidate, SetupSelection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfoDto {
    pub id: String,
    pub cwd: String,
    pub model: String,
    pub access_mode: AccessMode,
    pub reasoning_effort: ReasoningEffort,
    pub path: String,
    pub record_count: usize,
}

impl From<cassady::embedding::ConversationInfo> for ConversationInfoDto {
    fn from(info: cassady::embedding::ConversationInfo) -> Self {
        Self {
            id: info.id,
            cwd: info.cwd.display().to_string(),
            model: info.model,
            access_mode: info.access_mode,
            reasoning_effort: info.reasoning_effort,
            path: info.path.display().to_string(),
            record_count: info.record_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummaryDto {
    pub id: String,
    pub created_at: String,
    pub model: String,
    pub cwd: String,
    pub first_user_preview: String,
}

impl From<cassady::conversation::ChatSummary> for ChatSummaryDto {
    fn from(s: cassady::conversation::ChatSummary) -> Self {
        Self {
            id: s.id,
            created_at: s.created_at,
            model: s.model,
            cwd: s.cwd,
            first_user_preview: s.first_user_preview,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOptionDto {
    pub id: String,
    pub provider: String,
    pub display_name: Option<String>,
    pub reasoning_supported: bool,
    pub reasoning_required: bool,
    pub default_reasoning_effort: ReasoningEffort,
}

impl From<ModelDefinition> for ModelOptionDto {
    fn from(model: ModelDefinition) -> Self {
        Self {
            id: model.id,
            provider: model.provider,
            display_name: model.display_name,
            reasoning_supported: model.reasoning.supported,
            reasoning_required: model.reasoning.required,
            default_reasoning_effort: model.reasoning.default_effort,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionSettingsArgs {
    pub chat_id: String,
    pub access_mode: Option<AccessMode>,
    pub model: Option<String>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionArgs {
    pub cwd: Option<String>,
    pub access_mode: Option<AccessMode>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSessionArgs {
    pub chat_id: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListChatsArgs {
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveArgs {
    pub turn_id: String,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdArgs {
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTurnArgs {
    pub turn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum StreamEvent {
    AssistantChunk {
        text: String,
    },
    ReasoningChunk {
        text: String,
    },
    ToolCallStarted {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolOutputChunk {
        id: String,
        name: String,
        stream: String,
        content: String,
    },
    ToolResult {
        id: String,
        name: String,
        ok: bool,
        content: String,
    },
    ApprovalRequested {
        request_id: String,
        tool_call_id: String,
        name: String,
        arguments: serde_json::Value,
        reason: String,
    },
    ApprovalResolved {
        request_id: String,
        approved: bool,
    },
    Status {
        text: String,
    },
    Finished,
    Error {
        message: String,
    },
}

impl StreamEvent {
    pub fn from_embedding(event: cassady::embedding::Event) -> Self {
        match event {
            cassady::embedding::Event::AssistantChunk(text) => Self::AssistantChunk { text },
            cassady::embedding::Event::ReasoningChunk(text) => Self::ReasoningChunk { text },
            cassady::embedding::Event::ToolCallStarted {
                id,
                name,
                arguments,
            } => Self::ToolCallStarted {
                id,
                name,
                arguments,
            },
            cassady::embedding::Event::ToolOutputChunk {
                id,
                name,
                stream,
                content,
            } => Self::ToolOutputChunk {
                id,
                name,
                stream,
                content,
            },
            cassady::embedding::Event::ToolResult {
                id,
                name,
                ok,
                content,
            } => Self::ToolResult {
                id,
                name,
                ok,
                content,
            },
            cassady::embedding::Event::ApprovalRequested(req) => Self::ApprovalRequested {
                request_id: req.request_id,
                tool_call_id: req.tool_call_id,
                name: req.name,
                arguments: req.arguments,
                reason: req.reason,
            },
            cassady::embedding::Event::ApprovalResolved {
                request_id,
                approved,
            } => Self::ApprovalResolved {
                request_id,
                approved,
            },
            cassady::embedding::Event::Status(text) => Self::Status { text },
            cassady::embedding::Event::Finished => Self::Finished,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnHandle {
    pub turn_id: String,
    pub chat_id: String,
}

// --- Slash command DTOs -----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpecDto {
    pub name: String,
    pub usage: String,
    pub description: String,
    pub takes_value: bool,
}

impl From<CommandSpec> for CommandSpecDto {
    fn from(spec: CommandSpec) -> Self {
        Self {
            name: spec.name.to_string(),
            usage: spec.usage.to_string(),
            description: spec.description.to_string(),
            takes_value: spec.takes_value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFillItemDto {
    pub label: String,
    pub insert: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFillMenuDto {
    pub title: String,
    pub replacement_start: usize,
    pub replacement_end: usize,
    pub items: Vec<AutoFillItemDto>,
    pub selected: usize,
}

impl From<cassady::commands::AutoFillMenu> for AutoFillMenuDto {
    fn from(menu: cassady::commands::AutoFillMenu) -> Self {
        Self {
            title: menu.title,
            replacement_start: menu.replacement_start,
            replacement_end: menu.replacement_end,
            items: menu
                .items
                .into_iter()
                .map(|item| AutoFillItemDto {
                    label: item.label,
                    insert: item.insert,
                    detail: item.detail,
                })
                .collect(),
            selected: menu.selected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryDto {
    pub id: String,
    pub created_at: String,
    pub parent_chat_id: Option<String>,
    pub branch_label: Option<String>,
    pub record_count: usize,
    pub current: bool,
}

impl From<BranchSummary> for BranchSummaryDto {
    fn from(b: BranchSummary) -> Self {
        Self {
            id: b.id,
            created_at: b.created_at,
            parent_chat_id: b.parent_chat_id,
            branch_label: b.branch_label,
            record_count: b.record_count,
            current: b.current,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointDto {
    pub id: String,
    pub chat_id: String,
    pub record_index: usize,
    pub tool_call_id: Option<String>,
    pub kind: CheckpointKind,
    pub label: String,
    pub detail: String,
    pub ts: Option<String>,
}

impl From<Checkpoint> for CheckpointDto {
    fn from(c: Checkpoint) -> Self {
        Self {
            id: c.id,
            chat_id: c.chat_id,
            record_index: c.record_index,
            tool_call_id: c.tool_call_id,
            kind: c.kind,
            label: c.label,
            detail: c.detail,
            ts: c.ts,
        }
    }
}

impl From<CheckpointDto> for Checkpoint {
    fn from(c: CheckpointDto) -> Self {
        Self {
            id: c.id,
            chat_id: c.chat_id,
            record_index: c.record_index,
            tool_call_id: c.tool_call_id,
            kind: c.kind,
            label: c.label,
            detail: c.detail,
            ts: c.ts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchFamilyDto {
    pub branches: Vec<BranchSummaryDto>,
    pub checkpoints: Vec<CheckpointDto>,
}

impl From<BranchFamily> for BranchFamilyDto {
    fn from(f: BranchFamily) -> Self {
        Self {
            branches: f.branches.into_iter().map(BranchSummaryDto::from).collect(),
            checkpoints: f.checkpoints.into_iter().map(CheckpointDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLogoutCandidateDto {
    pub id: String,
    pub name: Option<String>,
    pub default_model: Option<String>,
    pub model_count: usize,
}

impl From<ProviderLogoutCandidate> for ProviderLogoutCandidateDto {
    fn from(c: ProviderLogoutCandidate) -> Self {
        Self {
            id: c.id,
            name: c.name,
            default_model: c.default_model,
            model_count: c.model_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutResultDto {
    pub removed_provider_ids: Vec<String>,
    pub removed_model_count: usize,
    pub remaining_provider_count: usize,
    pub active_provider: Option<String>,
    pub active_model: Option<String>,
}

impl From<LogoutResult> for LogoutResultDto {
    fn from(r: LogoutResult) -> Self {
        Self {
            removed_provider_ids: r.removed_provider_ids,
            removed_model_count: r.removed_model_count,
            remaining_provider_count: r.remaining_provider_count,
            active_provider: r.active_provider,
            active_model: r.active_model,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogEntryDto {
    pub name: String,
    pub id: String,
    pub base_url: String,
    pub api_key_env: String,
}

impl From<ProviderCatalogEntry> for ProviderCatalogEntryDto {
    fn from(e: ProviderCatalogEntry) -> Self {
        Self {
            name: e.name.to_string(),
            id: e.id.to_string(),
            base_url: e.base_url.to_string(),
            api_key_env: e.api_key_env.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSelectionDto {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key_env: String,
    pub model_id: String,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
}

impl From<SetupSelectionDto> for SetupSelection {
    fn from(s: SetupSelectionDto) -> Self {
        Self {
            provider_id: s.provider_id,
            provider_name: s.provider_name,
            base_url: s.base_url,
            api_key_env: s.api_key_env,
            model_id: s.model_id,
            supports_tools: s.supports_tools,
            supports_reasoning: s.supports_reasoning,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReportDto {
    pub summary: String,
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchResultDto {
    pub info: ConversationInfoDto,
    pub source_chat_id: String,
    pub status: String,
    pub restore: Option<RestoreReportDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderApplyResultDto {
    pub active_provider: String,
    pub active_model: String,
}

/// Tagged result of `run_slash_command`, mirroring `cassady::commands::CommandOutcome`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CommandOutcomeDto {
    Status {
        title: String,
        content: String,
    },
    NewChat {
        info: ConversationInfoDto,
        status: String,
    },
    ResumedChat {
        info: ConversationInfoDto,
        warning: Option<String>,
        status: String,
    },
    OpenBranchPicker {
        family: BranchFamilyDto,
    },
    OpenLoginWizard,
    OpenLogoutPicker {
        candidates: Vec<ProviderLogoutCandidateDto>,
    },
    Busy {
        message: String,
    },
    ParseError {
        message: String,
    },
    Error {
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSlashCommandArgs {
    pub chat_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashAutofillArgs {
    pub input: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBranchArgs {
    pub chat_id: String,
    pub checkpoint: CheckpointDto,
    pub restore_files: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProviderLoginArgs {
    pub selections: Vec<SetupSelectionDto>,
    pub active_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverModelsArgs {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProvidersArgs {
    pub provider_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_event_serializes_camel_case_with_kind_tag() {
        let ev = StreamEvent::AssistantChunk { text: "hi".into() };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["kind"], "assistantChunk");
        assert_eq!(json["text"], "hi");

        let ev = StreamEvent::ToolResult {
            id: "1".into(),
            name: "ls".into(),
            ok: true,
            content: "x".into(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["kind"], "toolResult");
        assert_eq!(json["ok"], true);
    }

    #[test]
    fn from_embedding_maps_each_variant() {
        use cassady::embedding::Event;
        let cases: Vec<(Event, &str)> = vec![
            (Event::AssistantChunk("a".into()), "assistantChunk"),
            (Event::ReasoningChunk("r".into()), "reasoningChunk"),
            (
                Event::ToolCallStarted {
                    id: "1".into(),
                    name: "ls".into(),
                    arguments: serde_json::Value::Null,
                },
                "toolCallStarted",
            ),
            (Event::Status("s".into()), "status"),
            (Event::Finished, "finished"),
        ];
        for (ev, expected_kind) in cases {
            let mapped = StreamEvent::from_embedding(ev);
            let json = serde_json::to_value(&mapped).unwrap();
            assert_eq!(json["kind"], expected_kind);
        }
    }
}
