use crate::conversation::StoredToolCall;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ModelMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
        #[serde(default)]
        reasoning: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_field: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<StoredToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct CompletionResult {
    pub content: String,
    pub reasoning: String,
    pub reasoning_field: Option<String>,
    pub tool_calls: Vec<StoredToolCall>,
}
