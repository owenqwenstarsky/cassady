use crate::access::AccessMode;
use crate::config::{Config, ReasoningEffort};
use crate::conversation::{now_ts, Conversation, Record, StoredToolCall};
use crate::prompt;
use crate::providers::openai_compatible::{OpenAiCompatibleProvider, OpenAiCompatibleSettings};
use crate::providers::types::ModelMessage;
use crate::tools::{self, ToolContext};
use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AssistantChunk(String),
    ReasoningChunk(String),
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
    pub reasoning_effort: ReasoningEffort,
}

const EMPTY_FINAL_RETRY_PROMPT: &str = "The previous response contained no user-facing text. Provide a concise final user-facing response summarizing the outcome. Do not call tools unless absolutely necessary.";

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

    let api_key = match settings.config.resolved_api_key() {
        Ok(api_key) => api_key,
        Err(err) => {
            append_visible_assistant(
                &mut conversation,
                &tx,
                format!("I couldn't start the turn because the API key is not available: {err}"),
            )?;
            let _ = tx.send(AgentEvent::TurnFinished);
            return Ok(conversation);
        }
    };
    let reasoning_request_format = settings
        .config
        .model_metadata
        .as_ref()
        .map(|model| model.reasoning.request_format)
        .unwrap_or_default();
    let reasoning_effort = settings
        .reasoning_effort
        .clamp_for_model(settings.config.model_metadata.as_ref());
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleSettings {
        model: settings.config.model.clone(),
        base_url: settings.config.active_provider.base_url.clone(),
        api_key,
        reasoning_effort,
        reasoning_request_format,
    });

    let docs_dir = settings.config.docs_dir();
    let tool_ctx = ToolContext {
        mode: settings.mode,
        cwd: settings.cwd.clone(),
        read_roots: vec![settings.cwd.clone(), docs_dir.clone()],
        blocked_write_roots: vec![docs_dir.clone()],
        model_result_limit: settings.config.model_tool_result_limit,
    };

    let mut retrying_empty_final = false;
    loop {
        let allowed = tools::available_tool_names(settings.mode);
        let system = prompt::build_effective_system_prompt(
            &conversation.base_system_prompt(),
            settings.mode,
            &settings.cwd,
            &docs_dir,
            &settings.config.model,
            &allowed,
        );
        let mut messages = build_messages(
            &conversation.records,
            system,
            settings.config.context_message_limit,
        );
        if retrying_empty_final {
            messages.push(ModelMessage::User {
                content: EMPTY_FINAL_RETRY_PROMPT.to_string(),
            });
        }
        let completion = match provider
            .complete(messages, tools::specs(settings.mode), &tx)
            .await
        {
            Ok(c) => c,
            Err(err) => {
                append_visible_assistant(
                    &mut conversation,
                    &tx,
                    format!("I couldn't complete the turn because the provider returned an error: {err}"),
                )?;
                break;
            }
        };

        let tool_calls = completion.tool_calls.clone();
        if tool_calls.is_empty() && completion.content.trim().is_empty() {
            if !retrying_empty_final {
                retrying_empty_final = true;
                let _ = tx.send(AgentEvent::Status(
                    "model returned an empty final response; requesting a final message".into(),
                ));
                continue;
            }
            append_visible_assistant(
                &mut conversation,
                &tx,
                "The model finished without a final response.".into(),
            )?;
            break;
        }
        retrying_empty_final = false;

        conversation.append(Record::Assistant {
            content: completion.content,
            reasoning: completion.reasoning,
            reasoning_field: completion.reasoning_field,
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

fn append_visible_assistant(
    conversation: &mut Conversation,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    content: String,
) -> Result<()> {
    let _ = tx.send(AgentEvent::AssistantChunk(content.clone()));
    conversation.append(Record::Assistant {
        content,
        reasoning: String::new(),
        reasoning_field: None,
        tool_calls: Vec::new(),
        ts: now_ts(),
    })
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
                reasoning,
                reasoning_field,
                tool_calls,
                ..
            } => non_system.push(ModelMessage::Assistant {
                content: content.clone(),
                reasoning: reasoning.clone(),
                reasoning_field: reasoning_field.clone(),
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
