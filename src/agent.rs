use crate::access::AccessMode;
use crate::config::Config;
use crate::conversation::{now_ts, Conversation, Record, StoredToolCall};
use crate::prompt;
use crate::providers::openai_compatible::OpenAiCompatibleProvider;
use crate::providers::types::ModelMessage;
use crate::tools::{self, ToolContext};
use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AssistantChunk(String),
    ToolCallStarted {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        name: String,
        ok: bool,
        content: String,
    },
    Status(String),
    TurnFinished,
}

#[derive(Debug, Clone)]
pub struct AgentSettings {
    pub config: Config,
    pub cwd: PathBuf,
    pub mode: AccessMode,
}

pub async fn run_turn(
    mut conversation: Conversation,
    user_message: String,
    settings: AgentSettings,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> Result<Conversation> {
    conversation.append(Record::User {
        content: user_message,
        ts: now_ts(),
    })?;

    let provider = match OpenAiCompatibleProvider::new(
        settings.config.model.clone(),
        settings.config.base_url.clone(),
        settings.config.api_key_env.clone(),
    ) {
        Ok(p) => p,
        Err(err) => {
            let _ = tx.send(AgentEvent::Status(err.to_string()));
            let _ = tx.send(AgentEvent::TurnFinished);
            return Ok(conversation);
        }
    };

    let tool_ctx = ToolContext {
        mode: settings.mode,
        cwd: settings.cwd.clone(),
        read_only_root: settings.cwd.clone(),
        model_result_limit: settings.config.model_tool_result_limit,
    };

    loop {
        let allowed = tools::available_tool_names(settings.mode);
        let system = prompt::build_effective_system_prompt(
            &conversation.base_system_prompt(),
            settings.mode,
            &settings.cwd,
            &settings.config.model,
            &allowed,
        );
        let messages = build_messages(
            &conversation.records,
            system,
            settings.config.context_message_limit,
        );
        let completion = match provider
            .complete(messages, tools::specs(settings.mode), &tx)
            .await
        {
            Ok(c) => c,
            Err(err) => {
                let _ = tx.send(AgentEvent::Status(format!("provider error: {err}")));
                break;
            }
        };

        let tool_calls = completion.tool_calls.clone();
        conversation.append(Record::Assistant {
            content: completion.content,
            tool_calls: tool_calls.clone(),
            ts: now_ts(),
        })?;

        if tool_calls.is_empty() {
            break;
        }
        for call in tool_calls {
            let _ = tx.send(AgentEvent::ToolCallStarted {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            let output = tools::execute(&call.name, call.arguments.clone(), &tool_ctx).await;
            let _ = tx.send(AgentEvent::ToolResult {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: output.ok,
                content: output.content.clone(),
            });
            conversation.append(Record::Tool {
                tool_call_id: call.id,
                name: call.name,
                ok: output.ok,
                content: output.content,
                ts: now_ts(),
            })?;
        }
    }
    let _ = tx.send(AgentEvent::TurnFinished);
    Ok(conversation)
}

fn build_messages(records: &[Record], system: String, limit: usize) -> Vec<ModelMessage> {
    let mut non_system = Vec::new();
    for r in records {
        match r {
            Record::User { content, .. } => non_system.push(ModelMessage::User {
                content: content.clone(),
            }),
            Record::Assistant {
                content,
                tool_calls,
                ..
            } => non_system.push(ModelMessage::Assistant {
                content: content.clone(),
                tool_calls: tool_calls.clone(),
            }),
            Record::Tool {
                tool_call_id,
                name,
                content,
                ..
            } => non_system.push(ModelMessage::Tool {
                tool_call_id: tool_call_id.clone(),
                name: name.clone(),
                content: content.clone(),
            }),
            _ => {}
        }
    }
    if non_system.len() > limit {
        non_system = non_system.split_off(non_system.len() - limit);
    }
    let mut messages = vec![ModelMessage::System { content: system }];
    messages.extend(non_system);
    repair_tool_message_prefix(messages)
}

fn repair_tool_message_prefix(mut messages: Vec<ModelMessage>) -> Vec<ModelMessage> {
    // If context trimming begins with tool results, remove them because OpenAI-compatible APIs
    // require tool messages to follow an assistant tool call.
    while matches!(messages.get(1), Some(ModelMessage::Tool { .. })) {
        messages.remove(1);
    }
    messages
}

#[allow(dead_code)]
fn _calls(_calls: Vec<StoredToolCall>) {}
