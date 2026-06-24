use crate::access::AccessMode;
use crate::config::{Config, ReasoningEffort};
use crate::conversation::{now_ts, Conversation, Record, StoredToolCall};
use crate::prompt;
use crate::providers::openai_compatible::{OpenAiCompatibleProvider, OpenAiCompatibleSettings};
use crate::providers::types::ModelMessage;
use crate::tools::{self, ToolContext, ToolRuntimeEvent};
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeSet;
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
        runtime_tx: None,
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
        let mut messages = build_messages(&conversation.records, system, &settings.config);
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
            let call_id = call.id.clone();
            let call_name = call.name.clone();
            let call_arguments = call.arguments.clone();
            let _ = tx.send(AgentEvent::ToolCallStarted {
                id: call_id.clone(),
                name: call_name.clone(),
                arguments: call_arguments.clone(),
            });
            let (runtime_tx, mut runtime_rx) = mpsc::unbounded_channel::<ToolRuntimeEvent>();
            let mut call_tool_ctx = tool_ctx.clone();
            call_tool_ctx.runtime_tx = Some(runtime_tx);
            let output = {
                let execute = tools::execute(&call_name, call_arguments, &call_tool_ctx);
                tokio::pin!(execute);
                let output = loop {
                    tokio::select! {
                        output = &mut execute => break output,
                        Some(event) = runtime_rx.recv() => {
                            forward_tool_runtime_event(&tx, &call_id, &call_name, event);
                        }
                    }
                };
                while let Ok(event) = runtime_rx.try_recv() {
                    forward_tool_runtime_event(&tx, &call_id, &call_name, event);
                }
                output
            };
            let _ = tx.send(AgentEvent::ToolResult {
                id: call_id.clone(),
                name: call_name.clone(),
                ok: output.ok,
                content: output.content.clone(),
            });
            conversation.append(Record::Tool {
                tool_call_id: call_id,
                name: call_name,
                ok: output.ok,
                content: output.content,
                ts: now_ts(),
            })?;
        }
    }
    let _ = tx.send(AgentEvent::TurnFinished);
    Ok(conversation)
}

fn forward_tool_runtime_event(
    tx: &mpsc::UnboundedSender<AgentEvent>,
    id: &str,
    name: &str,
    event: ToolRuntimeEvent,
) {
    match event {
        ToolRuntimeEvent::OutputChunk { stream, content } => {
            let _ = tx.send(AgentEvent::ToolOutputChunk {
                id: id.to_string(),
                name: name.to_string(),
                stream,
                content,
            });
        }
    }
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

const FALLBACK_CONTEXT_TOKENS: usize = 32_000;
const FALLBACK_OUTPUT_RESERVE_TOKENS: usize = 4_096;
const MIN_INPUT_BUDGET_TOKENS: usize = 1_024;
const TOOL_OUTPUT_COMPACT_CHARS: usize = 1_200;
const TOOL_OUTPUT_TINY_CHARS: usize = 320;

fn build_messages(records: &[Record], system: String, config: &Config) -> Vec<ModelMessage> {
    let mut messages = vec![ModelMessage::System { content: system }];
    messages.extend(records.iter().filter_map(record_to_model_message));
    messages = sanitize_tool_message_structure(messages);

    let budget = context_budget_tokens(config);
    if estimate_messages_tokens(&messages) > budget {
        compact_tool_outputs(&mut messages, budget);
        messages = trim_to_context_budget(messages, budget);
    }
    messages = trim_to_message_limit(messages, config.context_message_limit.max(1));
    sanitize_tool_message_structure(messages)
}

fn record_to_model_message(record: &Record) -> Option<ModelMessage> {
    match record {
        Record::User { content, .. } => Some(ModelMessage::User {
            content: content.clone(),
        }),
        Record::Assistant {
            content,
            reasoning,
            reasoning_field,
            tool_calls,
            ..
        } => Some(ModelMessage::Assistant {
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
        } => Some(ModelMessage::Tool {
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            content: content.clone(),
        }),
        _ => None,
    }
}

fn context_budget_tokens(config: &Config) -> usize {
    let context_length = config
        .model_metadata
        .as_ref()
        .and_then(|m| m.context_length)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(FALLBACK_CONTEXT_TOKENS)
        .max(MIN_INPUT_BUDGET_TOKENS * 2);
    let output_reserve = config
        .model_metadata
        .as_ref()
        .and_then(|m| m.max_output_tokens)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(FALLBACK_OUTPUT_RESERVE_TOKENS)
        .min(context_length / 2);
    let safety_reserve = (context_length / 50).clamp(512, 8_192);
    context_length
        .saturating_sub(output_reserve + safety_reserve)
        .max(MIN_INPUT_BUDGET_TOKENS)
}

fn compact_tool_outputs(messages: &mut [ModelMessage], budget: usize) {
    let newest_tool_idx = messages
        .iter()
        .rposition(|message| matches!(message, ModelMessage::Tool { .. }));

    for idx in 1..messages.len() {
        if Some(idx) == newest_tool_idx {
            continue;
        }
        compact_tool_output_at(messages, idx, TOOL_OUTPUT_COMPACT_CHARS);
        if estimate_messages_tokens(messages) <= budget {
            return;
        }
    }

    for idx in 1..messages.len() {
        compact_tool_output_at(messages, idx, TOOL_OUTPUT_TINY_CHARS);
        if estimate_messages_tokens(messages) <= budget {
            return;
        }
    }
}

fn compact_tool_output_at(messages: &mut [ModelMessage], idx: usize, target_chars: usize) {
    let Some(ModelMessage::Tool { content, .. }) = messages.get_mut(idx) else {
        return;
    };
    if content.chars().count() <= target_chars {
        return;
    }
    *content = compact_text(content, target_chars);
}

fn compact_text(content: &str, target_chars: usize) -> String {
    let original_chars = content.chars().count();
    let head_chars = (target_chars * 2 / 3).max(1);
    let tail_chars = target_chars.saturating_sub(head_chars).max(1);
    let head: String = content.chars().take(head_chars).collect();
    let tail: String = content
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!(
        "[Cass compacted this tool output from {original_chars} chars to fit the model context. Head/tail excerpt follows.]\n{head}\n… omitted …\n{tail}"
    )
}

fn trim_to_context_budget(mut messages: Vec<ModelMessage>, budget: usize) -> Vec<ModelMessage> {
    let mut omitted = false;
    while estimate_messages_tokens(&messages) > budget && messages.len() > 1 {
        let prefix_end = safe_prefix_end(&messages);
        if prefix_end <= 1 {
            break;
        }
        messages.drain(1..prefix_end);
        omitted = true;
    }

    if omitted {
        let note = ModelMessage::System {
            content: "Cass omitted earlier conversation messages to fit the model context budget. Included tool results still follow their matching assistant tool calls; older large tool outputs may be compacted.".to_string(),
        };
        messages.insert(1, note);
        while estimate_messages_tokens(&messages) > budget && messages.len() > 2 {
            let prefix_end = safe_prefix_end(&messages);
            if prefix_end <= 1 {
                break;
            }
            messages.drain(1..prefix_end);
        }
    }

    messages
}

fn trim_to_message_limit(mut messages: Vec<ModelMessage>, limit: usize) -> Vec<ModelMessage> {
    while messages.len().saturating_sub(1) > limit {
        let prefix_end = safe_prefix_end(&messages);
        if prefix_end <= 1 {
            break;
        }
        messages.drain(1..prefix_end);
    }
    messages
}

fn safe_prefix_end(messages: &[ModelMessage]) -> usize {
    let Some(first) = messages.get(1) else {
        return 1;
    };
    match first {
        ModelMessage::Tool { .. } => {
            let mut end = 2;
            while matches!(messages.get(end), Some(ModelMessage::Tool { .. })) {
                end += 1;
            }
            end
        }
        ModelMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
            let expected: BTreeSet<&str> = tool_calls.iter().map(|call| call.id.as_str()).collect();
            let mut seen = BTreeSet::new();
            let mut end = 2;
            while let Some(ModelMessage::Tool { tool_call_id, .. }) = messages.get(end) {
                if !expected.contains(tool_call_id.as_str()) {
                    break;
                }
                seen.insert(tool_call_id.as_str());
                end += 1;
                if seen == expected {
                    break;
                }
            }
            end
        }
        _ => 2,
    }
}

fn sanitize_tool_message_structure(messages: Vec<ModelMessage>) -> Vec<ModelMessage> {
    let mut out = Vec::with_capacity(messages.len());
    let mut iter = messages.into_iter();
    if let Some(system) = iter.next() {
        out.push(system);
    }
    let rest: Vec<_> = iter.collect();
    let mut idx = 0;
    while idx < rest.len() {
        match &rest[idx] {
            ModelMessage::Tool { .. } => {
                idx += 1;
            }
            ModelMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
                let expected: BTreeSet<&str> =
                    tool_calls.iter().map(|call| call.id.as_str()).collect();
                let mut seen = BTreeSet::new();
                let mut end = idx + 1;
                while let Some(ModelMessage::Tool { tool_call_id, .. }) = rest.get(end) {
                    if !expected.contains(tool_call_id.as_str()) {
                        break;
                    }
                    seen.insert(tool_call_id.as_str());
                    end += 1;
                    if seen == expected {
                        break;
                    }
                }

                if seen == expected {
                    out.extend(rest[idx..end].iter().cloned());
                } else if let ModelMessage::Assistant {
                    content,
                    reasoning,
                    reasoning_field,
                    ..
                } = &rest[idx]
                {
                    if !content.trim().is_empty() || !reasoning.trim().is_empty() {
                        out.push(ModelMessage::Assistant {
                            content: content.clone(),
                            reasoning: reasoning.clone(),
                            reasoning_field: reasoning_field.clone(),
                            tool_calls: Vec::new(),
                        });
                    }
                    while end < rest.len()
                        && matches!(rest.get(end), Some(ModelMessage::Tool { .. }))
                    {
                        end += 1;
                    }
                }
                idx = end;
            }
            message => {
                out.push(message.clone());
                idx += 1;
            }
        }
    }
    out
}

fn estimate_messages_tokens(messages: &[ModelMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &ModelMessage) -> usize {
    const MESSAGE_OVERHEAD: usize = 8;
    match message {
        ModelMessage::System { content } | ModelMessage::User { content } => {
            MESSAGE_OVERHEAD + estimate_text_tokens(content)
        }
        ModelMessage::Assistant {
            content,
            reasoning,
            reasoning_field,
            tool_calls,
        } => {
            MESSAGE_OVERHEAD
                + estimate_text_tokens(content)
                + estimate_text_tokens(reasoning)
                + reasoning_field
                    .as_deref()
                    .map(estimate_text_tokens)
                    .unwrap_or(0)
                + tool_calls
                    .iter()
                    .map(|call| {
                        estimate_text_tokens(&call.id)
                            + estimate_text_tokens(&call.name)
                            + estimate_text_tokens(&call.arguments.to_string())
                            + 12
                    })
                    .sum::<usize>()
        }
        ModelMessage::Tool {
            tool_call_id,
            name,
            content,
        } => {
            MESSAGE_OVERHEAD
                + estimate_text_tokens(tool_call_id)
                + estimate_text_tokens(name)
                + estimate_text_tokens(content)
        }
    }
}

fn estimate_text_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

#[allow(dead_code)]
fn _calls(_calls: Vec<StoredToolCall>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_model_definition;
    use serde_json::json;

    fn small_config(context_length: u64, max_output_tokens: u64) -> Config {
        let mut config = Config::default();
        let mut model = default_model_definition();
        model.context_length = Some(context_length);
        model.max_output_tokens = Some(max_output_tokens);
        config.model_metadata = Some(model);
        config
    }

    fn call(id: &str) -> StoredToolCall {
        StoredToolCall {
            id: id.to_string(),
            name: "read".to_string(),
            arguments: json!({"path":"src/main.rs"}),
        }
    }

    fn assert_valid_tool_structure(messages: &[ModelMessage]) {
        let mut idx = 1;
        while idx < messages.len() {
            match &messages[idx] {
                ModelMessage::Tool { .. } => panic!("orphaned tool message at {idx}"),
                ModelMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty() => {
                    let expected: BTreeSet<&str> =
                        tool_calls.iter().map(|call| call.id.as_str()).collect();
                    let mut seen = BTreeSet::new();
                    idx += 1;
                    while let Some(ModelMessage::Tool { tool_call_id, .. }) = messages.get(idx) {
                        assert!(expected.contains(tool_call_id.as_str()));
                        seen.insert(tool_call_id.as_str());
                        idx += 1;
                        if seen == expected {
                            break;
                        }
                    }
                    assert_eq!(seen, expected);
                }
                _ => idx += 1,
            }
        }
    }

    #[test]
    fn context_budget_compacts_large_tool_outputs_before_dropping_records() {
        let records = vec![
            Record::User {
                content: "please inspect this file".into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call("call_1")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_1".into(),
                name: "read".into(),
                ok: true,
                content: "x".repeat(32_000),
                ts: now_ts(),
            },
            Record::Assistant {
                content: "I inspected it.".into(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: Vec::new(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &small_config(8_000, 512));

        assert_valid_tool_structure(&messages);
        assert!(messages.iter().any(|message| matches!(
            message,
            ModelMessage::Tool { content, .. } if content.contains("Cass compacted this tool output")
        )));
    }

    #[test]
    fn context_budget_trimming_does_not_leave_orphaned_tool_results() {
        let records = vec![
            Record::User {
                content: "old".repeat(8_000),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call("call_1")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_1".into(),
                name: "read".into(),
                ok: true,
                content: "result".repeat(2_000),
                ts: now_ts(),
            },
            Record::User {
                content: "new question".into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &small_config(2_000, 512));

        assert_valid_tool_structure(&messages);
        assert!(
            matches!(messages.last(), Some(ModelMessage::User { content }) if content == "new question")
        );
    }

    #[test]
    fn invalid_leading_tool_results_are_removed() {
        let messages = sanitize_tool_message_structure(vec![
            ModelMessage::System {
                content: "system".into(),
            },
            ModelMessage::Tool {
                tool_call_id: "missing".into(),
                name: "read".into(),
                content: "orphan".into(),
            },
            ModelMessage::User {
                content: "hello".into(),
            },
        ]);

        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[1], ModelMessage::User { .. }));
    }
}
