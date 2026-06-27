use cassady::access::AccessMode;
use cassady::config::{ModelDefinition, ReasoningEffort};
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
