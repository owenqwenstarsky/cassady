use crate::access::AccessMode;
use crate::config::{Config, ReasoningEffort};
use crate::conversation::{now_ts, Conversation, Record, StoredToolCall};
use crate::prompt;
use crate::providers::types::ModelMessage;
use crate::providers::{ProviderClient, ProviderRuntimeOptions};
use crate::security::PolicyDecision;
use crate::tools::{self, ToolContext, ToolRuntimeEvent};
use anyhow::Result;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
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
    ApprovalRequested {
        request_id: String,
        tool_call_id: String,
        name: String,
        arguments: Value,
        reason: String,
    },
    ApprovalResolved {
        request_id: String,
        approved: bool,
    },
    Status(String),
    TurnFinished,
}

#[derive(Debug, Clone)]
pub enum AgentCommand {
    ApprovalDecision { request_id: String, approved: bool },
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
    conversation: Conversation,
    user_message: String,
    settings: AgentSettings,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> Result<Conversation> {
    let (_command_tx, command_rx) = mpsc::unbounded_channel();
    run_turn_with_commands(conversation, user_message, settings, tx, command_rx).await
}

pub async fn run_turn_with_commands(
    mut conversation: Conversation,
    user_message: String,
    settings: AgentSettings,
    tx: mpsc::UnboundedSender<AgentEvent>,
    mut command_rx: mpsc::UnboundedReceiver<AgentCommand>,
) -> Result<Conversation> {
    conversation.append(Record::User {
        content: user_message,
        ts: now_ts(),
    })?;

    let reasoning_effort = settings
        .reasoning_effort
        .clamp_for_model(settings.config.model_metadata.as_ref());
    let provider = match ProviderClient::from_config(
        &settings.config,
        ProviderRuntimeOptions {
            reasoning_effort,
            fast_mode: settings.config.fast_mode_state().active,
        },
    ) {
        Ok(provider) => provider,
        Err(err) => {
            append_visible_assistant(
                &mut conversation,
                &tx,
                format!("I couldn't start the turn because provider authentication is not available: {err}"),
            )?;
            let _ = tx.send(AgentEvent::TurnFinished);
            return Ok(conversation);
        }
    };

    let docs_dir = settings.config.docs_dir();
    let tool_ctx = ToolContext {
        mode: settings.mode,
        cwd: settings.cwd.clone(),
        read_roots: vec![settings.cwd.clone(), docs_dir.clone()],
        blocked_write_roots: vec![docs_dir.clone()],
        // Keep stored/UI tool results intact; build_messages applies the
        // model-facing result limit when preparing provider messages.
        model_result_limit: usize::MAX,
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
            .complete(
                messages,
                tools::specs_for_context(&tool_ctx.security_context()),
                &tx,
            )
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
            let mut approved =
                match tools::policy_decision_for_call(&call_name, &call_arguments, &tool_ctx) {
                    Some(PolicyDecision::Allow) | None => false,
                    Some(PolicyDecision::Deny { reason }) => {
                        let output = tools::ToolOutput {
                            ok: false,
                            content: reason,
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
                        continue;
                    }
                    Some(PolicyDecision::Ask { reason }) => {
                        let request_id = format!("approval_{}", nanoid::nanoid!(8));
                        let _ = tx.send(AgentEvent::ApprovalRequested {
                            request_id: request_id.clone(),
                            tool_call_id: call_id.clone(),
                            name: call_name.clone(),
                            arguments: call_arguments.clone(),
                            reason,
                        });
                        let approved = wait_for_approval(&mut command_rx, &request_id).await;
                        let _ = tx.send(AgentEvent::ApprovalResolved {
                            request_id: request_id.clone(),
                            approved,
                        });
                        if !approved {
                            let output = tools::ToolOutput {
                                ok: false,
                                content: "user denied approval for this tool call".into(),
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
                            continue;
                        }
                        true
                    }
                };
            if !approved {
                if let Some(reason) = destructive_confirmation_reason(
                    settings.mode,
                    settings.config.confirm_destructive_operations,
                    &call_name,
                    &call_arguments,
                    &settings.cwd,
                ) {
                    let request_id = format!("approval_{}", nanoid::nanoid!(8));
                    let _ = tx.send(AgentEvent::ApprovalRequested {
                        request_id: request_id.clone(),
                        tool_call_id: call_id.clone(),
                        name: call_name.clone(),
                        arguments: call_arguments.clone(),
                        reason,
                    });
                    approved = wait_for_approval(&mut command_rx, &request_id).await;
                    let _ = tx.send(AgentEvent::ApprovalResolved {
                        request_id,
                        approved,
                    });
                    if !approved {
                        let output = tools::ToolOutput {
                            ok: false,
                            content: "user denied approval for this destructive tool call".into(),
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
                        continue;
                    }
                }
            }
            let (runtime_tx, mut runtime_rx) = mpsc::unbounded_channel::<ToolRuntimeEvent>();
            let mut call_tool_ctx = tool_ctx.clone();
            call_tool_ctx.runtime_tx = Some(runtime_tx);
            let file_edit_snapshot = crate::file_edits::begin_tool_edit(
                &settings.config.root,
                &conversation.id,
                conversation.records.len(),
                &call_id,
                &call_name,
                &call_arguments,
                &call_tool_ctx,
            );
            let output = {
                let execute = tools::execute_with_approval(
                    &call_name,
                    call_arguments.clone(),
                    &call_tool_ctx,
                    approved,
                );
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
            if output.ok {
                if let Some(snapshot) = file_edit_snapshot {
                    let _ = crate::file_edits::finish_tool_edit(&settings.config.root, snapshot);
                }
            }
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

async fn wait_for_approval(
    command_rx: &mut mpsc::UnboundedReceiver<AgentCommand>,
    request_id: &str,
) -> bool {
    while let Some(command) = command_rx.recv().await {
        match command {
            AgentCommand::ApprovalDecision {
                request_id: id,
                approved,
            } if id == request_id => return approved,
            _ => {}
        }
    }
    false
}

fn destructive_confirmation_reason(
    mode: AccessMode,
    enabled: bool,
    name: &str,
    arguments: &Value,
    cwd: &Path,
) -> Option<String> {
    if !enabled || mode != AccessMode::FullAccess {
        return None;
    }
    match name {
        "write" => {
            let path = arguments.get("path")?.as_str()?;
            let p = crate::tools::path::expand_tilde(path);
            let abs = if p.is_absolute() { p } else { cwd.join(p) };
            if abs.exists() {
                Some(format!(
                    "write will overwrite an existing file in full-access mode: {}",
                    abs.display()
                ))
            } else {
                None
            }
        }
        "edit" => arguments
            .get("path")
            .and_then(Value::as_str)
            .map(|path| format!("edit will modify a file in full-access mode: {path}")),
        "shell" => {
            let command = arguments.get("command")?.as_str()?;
            if shell_command_looks_destructive(command) {
                Some(format!(
                    "shell command appears destructive and requires confirmation: {command}"
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn shell_command_looks_destructive(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    let risky = [
        "rm ",
        "rm -",
        "rmdir",
        "mv ",
        "chmod ",
        "chown ",
        "dd ",
        "mkfs",
        "truncate ",
        "shred",
        ">",
        "tee ",
        "git reset",
        "git clean",
        "drop table",
        "delete from",
    ];
    risky.iter().any(|needle| lowered.contains(needle))
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
    supersede_old_read_outputs(&mut messages);
    apply_model_tool_result_limits(&mut messages, config.model_tool_result_limit);

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

fn supersede_old_read_outputs(messages: &mut [ModelMessage]) {
    let read_calls = tool_calls_by_id(messages);
    let mut read_outputs = Vec::new();
    for (message_idx, message) in messages.iter().enumerate() {
        let ModelMessage::Tool {
            tool_call_id,
            name,
            content,
        } = message
        else {
            continue;
        };
        if name != "read" {
            continue;
        }
        let request_specs = read_calls
            .get(tool_call_id)
            .map(|call| read_request_specs(&call.arguments))
            .unwrap_or_default();
        let sections = parse_read_output_sections(content, &request_specs);
        if !sections.is_empty() {
            read_outputs.push(ReadOutputInfo {
                message_idx,
                sections,
            });
        }
    }

    for read_idx in 0..read_outputs.len() {
        let superseded: Vec<bool> = read_outputs[read_idx]
            .sections
            .iter()
            .map(|section| {
                read_outputs[read_idx + 1..]
                    .iter()
                    .flat_map(|later| later.sections.iter())
                    .any(|later| later.path == section.path && range_covers(later, section))
            })
            .collect();
        if !superseded.iter().any(|v| *v) {
            continue;
        }

        let message_idx = read_outputs[read_idx].message_idx;
        let replacement = superseded_read_content(
            tool_content(messages, message_idx),
            &read_outputs[read_idx].sections,
            &superseded,
        );
        if let Some(ModelMessage::Tool { content, .. }) = messages.get_mut(message_idx) {
            *content = replacement;
        }
    }
}

fn tool_content(messages: &[ModelMessage], idx: usize) -> &str {
    match &messages[idx] {
        ModelMessage::Tool { content, .. } => content,
        _ => "",
    }
}

fn tool_calls_by_id(messages: &[ModelMessage]) -> BTreeMap<String, StoredToolCall> {
    let mut calls = BTreeMap::new();
    for message in messages {
        let ModelMessage::Assistant { tool_calls, .. } = message else {
            continue;
        };
        for call in tool_calls {
            calls.insert(call.id.clone(), call.clone());
        }
    }
    calls
}

#[derive(Debug, Clone)]
struct ReadOutputInfo {
    message_idx: usize,
    sections: Vec<ReadOutputSection>,
}

#[derive(Debug, Clone)]
struct ReadOutputSection {
    path: String,
    start_line: usize,
    end_line: usize,
    coverage_start: usize,
    coverage_end: Option<usize>,
    start_byte: usize,
    end_byte: usize,
}

#[derive(Debug, Clone)]
struct ReadRequestSpec {
    start_line: usize,
    end_line: Option<usize>,
}

fn parse_read_output_sections(
    content: &str,
    request_specs: &[ReadRequestSpec],
) -> Vec<ReadOutputSection> {
    let mut headers = Vec::new();
    for (line_start, line) in content_lines_with_offsets(content) {
        if let Some((path, header_start, header_end)) = parse_read_header(line) {
            headers.push((line_start, path, header_start, header_end));
        }
    }

    let mut sections = Vec::new();
    for (idx, (start_byte, path, header_start, header_end)) in headers.iter().enumerate() {
        let end_byte = headers
            .get(idx + 1)
            .map(|(next_start, _, _, _)| *next_start)
            .unwrap_or(content.len());
        let section_content = &content[*start_byte..end_byte];
        let (start_line, end_line) =
            observed_read_range(section_content, *header_start, *header_end);
        let (coverage_start, coverage_end) = read_coverage(
            request_specs.get(idx),
            start_line,
            end_line,
            *header_end,
            section_content.contains("truncated by Cass"),
        );
        sections.push(ReadOutputSection {
            path: path.clone(),
            start_line,
            end_line,
            coverage_start,
            coverage_end,
            start_byte: *start_byte,
            end_byte,
        });
    }
    sections
}

fn read_request_specs(arguments: &Value) -> Vec<ReadRequestSpec> {
    if let Some(files) = arguments.get("files").and_then(|files| files.as_array()) {
        return files
            .iter()
            .map(|file| read_request_spec(file.get("lines").and_then(|lines| lines.as_str())))
            .collect();
    }

    if arguments.get("path").is_some() {
        return vec![read_request_spec(
            arguments.get("lines").and_then(|lines| lines.as_str()),
        )];
    }

    Vec::new()
}

fn read_request_spec(lines: Option<&str>) -> ReadRequestSpec {
    let Some(lines) = lines.map(str::trim).filter(|lines| !lines.is_empty()) else {
        return ReadRequestSpec {
            start_line: 1,
            end_line: None,
        };
    };
    let Some((start, end)) = lines.split_once('-') else {
        return ReadRequestSpec {
            start_line: 1,
            end_line: Some(0),
        };
    };
    let start_line = if start.is_empty() {
        1
    } else {
        start.parse().unwrap_or(1)
    };
    let end_line = if end.is_empty() {
        None
    } else {
        Some(end.parse().unwrap_or(0))
    };
    ReadRequestSpec {
        start_line,
        end_line,
    }
}

fn read_coverage(
    request: Option<&ReadRequestSpec>,
    observed_start: usize,
    observed_end: usize,
    header_end: usize,
    truncated: bool,
) -> (usize, Option<usize>) {
    let Some(request) = request.filter(|_| !truncated) else {
        return (observed_start, Some(observed_end));
    };

    let start = request.start_line.max(1);
    let end = match request.end_line {
        None => None,
        Some(requested_end) if header_end < requested_end => None,
        Some(requested_end) => Some(requested_end),
    };
    (start, end)
}

fn content_lines_with_offsets(content: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < content.len() {
        let rest = &content[start..];
        let newline_offset = rest.find('\n');
        let end = newline_offset
            .map(|offset| start + offset)
            .unwrap_or(content.len());
        out.push((start, &content[start..end]));
        start = match newline_offset {
            Some(offset) => start + offset + 1,
            None => content.len(),
        };
    }
    out
}

fn parse_read_header(line: &str) -> Option<(String, usize, usize)> {
    let inner = line.strip_prefix("--- ")?.strip_suffix(" ---")?;
    let (path, range) = inner.rsplit_once(" lines ")?;
    let (start, end) = range.split_once('-')?;
    Some((path.to_string(), start.parse().ok()?, end.parse().ok()?))
}

fn observed_read_range(
    section_content: &str,
    header_start: usize,
    header_end: usize,
) -> (usize, usize) {
    let observed: Vec<usize> = content_lines_with_offsets(section_content)
        .into_iter()
        .skip(1)
        .filter_map(|(_, line)| parse_numbered_read_line(line))
        .collect();
    match (observed.first(), observed.last()) {
        (Some(start), Some(end)) => (*start, *end),
        _ => (header_start, header_end),
    }
}

fn parse_numbered_read_line(line: &str) -> Option<usize> {
    let (prefix, _) = line.split_once(" | ")?;
    prefix.trim().parse().ok()
}

fn range_covers(newer: &ReadOutputSection, older: &ReadOutputSection) -> bool {
    if older.end_line < older.start_line {
        return newer.coverage_start <= older.start_line;
    }
    if newer.coverage_start > older.start_line {
        return false;
    }
    match newer.coverage_end {
        Some(end) => end >= older.end_line,
        None => true,
    }
}

fn superseded_read_content(
    original: &str,
    sections: &[ReadOutputSection],
    superseded: &[bool],
) -> String {
    let mut out = String::with_capacity(original.len().min(1024));
    let mut cursor = 0;
    for (section, is_superseded) in sections.iter().zip(superseded.iter()) {
        out.push_str(&original[cursor..section.start_byte]);
        if *is_superseded {
            out.push_str(&superseded_read_note(section));
        } else {
            out.push_str(&original[section.start_byte..section.end_byte]);
        }
        cursor = section.end_byte;
    }
    out.push_str(&original[cursor..]);
    out
}

fn superseded_read_note(section: &ReadOutputSection) -> String {
    format!(
        "[Cass omitted this earlier read output for {} lines {}-{} from the model context because a newer read of the same range exists later in the conversation.]\n",
        section.path, section.start_line, section.end_line
    )
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

#[derive(Debug, Clone, Copy)]
enum ToolOutputTransform {
    Compaction,
    Truncation,
}

impl ToolOutputTransform {
    fn verb(self) -> &'static str {
        match self {
            ToolOutputTransform::Compaction => "compacted",
            ToolOutputTransform::Truncation => "truncated",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            ToolOutputTransform::Compaction => "to fit the model context",
            ToolOutputTransform::Truncation => {
                "before sending it to the model because it exceeded the configured model_tool_result_limit"
            }
        }
    }
}

fn apply_model_tool_result_limits(messages: &mut [ModelMessage], limit_chars: usize) {
    let tool_calls = tool_calls_by_id(messages);
    let target_chars = limit_chars.max(1);
    for idx in 1..messages.len() {
        transform_tool_output_at(
            messages,
            idx,
            target_chars,
            ToolOutputTransform::Truncation,
            &tool_calls,
        );
    }
}

fn compact_tool_outputs(messages: &mut [ModelMessage], budget: usize) {
    let newest_tool_idx = messages
        .iter()
        .rposition(|message| matches!(message, ModelMessage::Tool { .. }));
    let tool_calls = tool_calls_by_id(messages);

    for idx in 1..messages.len() {
        if Some(idx) == newest_tool_idx {
            continue;
        }
        transform_tool_output_at(
            messages,
            idx,
            TOOL_OUTPUT_COMPACT_CHARS,
            ToolOutputTransform::Compaction,
            &tool_calls,
        );
        if estimate_messages_tokens(messages) <= budget {
            return;
        }
    }

    for idx in 1..messages.len() {
        transform_tool_output_at(
            messages,
            idx,
            TOOL_OUTPUT_TINY_CHARS,
            ToolOutputTransform::Compaction,
            &tool_calls,
        );
        if estimate_messages_tokens(messages) <= budget {
            return;
        }
    }
}

fn transform_tool_output_at(
    messages: &mut [ModelMessage],
    idx: usize,
    target_chars: usize,
    transform: ToolOutputTransform,
    tool_calls: &BTreeMap<String, StoredToolCall>,
) {
    let replacement = {
        let Some(ModelMessage::Tool {
            tool_call_id,
            name,
            content,
        }) = messages.get(idx)
        else {
            return;
        };
        if content.chars().count() <= target_chars.max(1) {
            return;
        }
        format_tool_output_transform(
            name,
            tool_calls.get(tool_call_id),
            content,
            target_chars,
            transform,
        )
    };

    if let Some(ModelMessage::Tool { content, .. }) = messages.get_mut(idx) {
        *content = replacement;
    }
}

fn format_tool_output_transform(
    tool_name: &str,
    call: Option<&StoredToolCall>,
    content: &str,
    target_chars: usize,
    transform: ToolOutputTransform,
) -> String {
    let original_chars = content.chars().count();
    if original_chars <= target_chars.max(1) {
        return content.to_string();
    }

    let target_chars = target_chars
        .max(1)
        .min(original_chars.saturating_sub(1).max(1));
    let head_chars = if target_chars <= 1 {
        1
    } else {
        (target_chars * 2 / 3).clamp(1, target_chars - 1)
    };
    let tail_chars = target_chars.saturating_sub(head_chars);
    let head: String = content.chars().take(head_chars).collect();
    let tail: String = content
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let notice = tool_output_transform_notice(
        tool_name,
        call,
        content,
        original_chars,
        head_chars,
        tail_chars,
        transform,
    );

    let mut out = String::new();
    out.push_str(&notice);
    out.push_str("\n--- retained head excerpt ---\n");
    out.push_str(&head);
    out.push_str("\n--- omitted middle ---\n");
    if tail_chars > 0 {
        out.push_str("--- retained tail excerpt ---\n");
        out.push_str(&tail);
    }
    out
}

fn tool_output_transform_notice(
    tool_name: &str,
    call: Option<&StoredToolCall>,
    content: &str,
    original_chars: usize,
    head_chars: usize,
    tail_chars: usize,
    transform: ToolOutputTransform,
) -> String {
    let retained_shape = if tail_chars > 0 {
        format!("{head_chars} chars from the start and {tail_chars} chars from the end")
    } else {
        format!("{head_chars} chars from the start")
    };
    let mut sentences = vec![format!(
        "Cass {} this tool output from {original_chars} chars {}. Tool: `{tool_name}`. Retained excerpt shape: {retained_shape}.",
        transform.verb(),
        transform.reason(),
    )];
    if let Some(provenance) = tool_output_provenance_sentence(tool_name, call, content) {
        sentences.push(provenance);
    }
    sentences.push(tool_output_recovery_guidance(tool_name).to_string());
    format!("[{}]", sentences.join(" "))
}

fn tool_output_provenance_sentence(
    tool_name: &str,
    call: Option<&StoredToolCall>,
    content: &str,
) -> Option<String> {
    match tool_name {
        "read" => read_output_provenance_sentence(call, content),
        "grep" => grep_output_provenance_sentence(content),
        "shell" => shell_output_provenance_sentence(call),
        _ => None,
    }
}

fn read_output_provenance_sentence(call: Option<&StoredToolCall>, content: &str) -> Option<String> {
    let request_specs = call
        .filter(|call| call.name == "read")
        .map(|call| read_request_specs(&call.arguments))
        .unwrap_or_default();
    let sections = parse_read_output_sections(content, &request_specs);
    if !sections.is_empty() {
        return Some(format!(
            "Omitted content came from {}.",
            read_sections_summary(&sections)
        ));
    }
    read_request_summary(call).map(|summary| format!("The read request targeted {summary}."))
}

fn read_sections_summary(sections: &[ReadOutputSection]) -> String {
    let labels: Vec<String> = sections.iter().take(2).map(read_section_label).collect();
    match sections.len() {
        0 => "no read sections".into(),
        1 => labels[0].clone(),
        2 => format!("2 read sections: {} and {}", labels[0], labels[1]),
        count => format!(
            "{count} read sections including {} and {}",
            labels[0], labels[1]
        ),
    }
}

fn read_section_label(section: &ReadOutputSection) -> String {
    format!(
        "{} lines {}-{}",
        section.path, section.start_line, section.end_line
    )
}

fn read_request_summary(call: Option<&StoredToolCall>) -> Option<String> {
    let call = call.filter(|call| call.name == "read")?;
    let mut labels = Vec::new();
    if let Some(files) = call
        .arguments
        .get("files")
        .and_then(|files| files.as_array())
    {
        for file in files.iter().take(2) {
            let Some(path) = file.get("path").and_then(|path| path.as_str()) else {
                continue;
            };
            labels.push(read_request_label(
                path,
                file.get("lines").and_then(|lines| lines.as_str()),
            ));
        }
        return match labels.len() {
            0 => None,
            1 => labels.first().cloned(),
            2 if files.len() == 2 => Some(format!("2 files: {} and {}", labels[0], labels[1])),
            _ => Some(format!(
                "{} files including {} and {}",
                files.len(),
                labels[0],
                labels[1]
            )),
        };
    }

    call.arguments
        .get("path")
        .and_then(|path| path.as_str())
        .map(|path| {
            read_request_label(
                path,
                call.arguments.get("lines").and_then(|lines| lines.as_str()),
            )
        })
}

fn read_request_label(path: &str, lines: Option<&str>) -> String {
    match lines.map(str::trim).filter(|lines| !lines.is_empty()) {
        Some(lines) => format!("{path} lines {lines}"),
        None => path.to_string(),
    }
}

fn grep_output_provenance_sentence(content: &str) -> Option<String> {
    grep_stopped_after(content)
        .map(|count| format!("The grep output reported it stopped after {count} matches."))
}

fn grep_stopped_after(content: &str) -> Option<usize> {
    content.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("… stopped after ")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

fn shell_output_provenance_sentence(call: Option<&StoredToolCall>) -> Option<String> {
    let call = call.filter(|call| call.name == "shell")?;
    let command = call.arguments.get("command")?.as_str()?.trim();
    if command.is_empty() {
        return None;
    }
    let preview = command.split_whitespace().collect::<Vec<_>>().join(" ");
    if command_preview_may_contain_sensitive_text(&preview) {
        return Some(
            "Shell command preview omitted because it may contain sensitive text; inspect the preceding tool-call arguments before rerunning."
                .into(),
        );
    }
    let chars = preview.chars().count();
    if chars > 160 {
        return Some(format!(
            "Shell command was {chars} chars; inspect the preceding tool-call arguments before rerunning."
        ));
    }
    Some(format!("Shell command: `{preview}`."))
}

fn command_preview_may_contain_sensitive_text(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn tool_output_recovery_guidance(tool_name: &str) -> &'static str {
    match tool_name {
        "read" => "Recovery: use `read` with a narrower line range, or `grep` for a symbol before reading, before relying on omitted details.",
        "grep" => "Recovery: rerun `grep` with a narrower query/path, lower `max_matches`, or `read` around specific matching lines before relying on omitted details.",
        "shell" => "Recovery: rerun a narrower command, filter output with grep/head/tail, or inspect specific files named in the excerpt before making edits from omitted lines.",
        _ => "Recovery: rerun a narrower tool request or inspect the specific file/range from the excerpt before relying on omitted details.",
    }
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
            content: "Cass omitted earlier conversation messages to fit the model context budget. Included tool results still follow their matching assistant tool calls; older large tool outputs may be compacted with recovery guidance.".to_string(),
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
    use serde_json::{json, Value};

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
            arguments: json!({"files":[{"path":"src/main.rs"}]}),
        }
    }

    fn call_with_lines(id: &str, path: &str, lines: &str) -> StoredToolCall {
        StoredToolCall {
            id: id.to_string(),
            name: "read".to_string(),
            arguments: json!({"files":[{"path":path,"lines":lines}]}),
        }
    }

    fn named_call(id: &str, name: &str, arguments: Value) -> StoredToolCall {
        StoredToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }
    }

    fn numbered_read_output(path: &str, start: usize, end: usize) -> String {
        let mut out = format!("--- {path} lines {start}-{end} ---\n");
        for line in start..=end {
            out.push_str(&format!("{line:>6} | line {line}\n"));
        }
        out
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
        let mut config = small_config(8_000, 512);
        config.model_tool_result_limit = 100_000;
        let messages = build_messages(&records, "system".into(), &config);

        assert_valid_tool_structure(&messages);
        assert!(messages.iter().any(|message| matches!(
            message,
            ModelMessage::Tool { content, .. } if content.contains("Cass compacted this tool output")
        )));
    }

    #[test]
    fn compacted_read_output_includes_range_and_reinspection_guidance() {
        let call = call_with_lines("call_1", "src/app.rs", "1-200");
        let content = numbered_read_output("src/app.rs", 1, 200);

        let compacted = format_tool_output_transform(
            "read",
            Some(&call),
            &content,
            320,
            ToolOutputTransform::Compaction,
        );

        assert!(compacted.contains("Cass compacted this tool output"));
        assert!(compacted.contains("Tool: `read`"));
        assert!(compacted.contains("Retained excerpt shape"));
        assert!(compacted.contains("src/app.rs lines 1-200"));
        assert!(compacted.contains("narrower line range"));
        assert!(compacted.contains("grep"));
        assert!(compacted.contains("--- retained head excerpt ---"));
        assert!(compacted.contains("--- retained tail excerpt ---"));
    }

    #[test]
    fn compacted_multi_file_read_output_summarizes_sections() {
        let call = named_call(
            "call_1",
            "read",
            json!({"files":[
                {"path":"src/a.rs","lines":"1-80"},
                {"path":"src/b.rs","lines":"20-90"}
            ]}),
        );
        let content = format!(
            "{}{}",
            numbered_read_output("src/a.rs", 1, 80),
            numbered_read_output("src/b.rs", 20, 90)
        );

        let compacted = format_tool_output_transform(
            "read",
            Some(&call),
            &content,
            360,
            ToolOutputTransform::Compaction,
        );

        assert!(compacted.contains("2 read sections"));
        assert!(compacted.contains("src/a.rs lines 1-80"));
        assert!(compacted.contains("src/b.rs lines 20-90"));
    }

    #[test]
    fn compacted_grep_output_adds_narrowing_guidance() {
        let call = named_call(
            "call_1",
            "grep",
            json!({"query":"needle","paths":["src"],"max_matches":300}),
        );
        let mut content = String::new();
        for line in 1..=300 {
            content.push_str(&format!("src/lib.rs:{line}: needle {line}\n"));
        }
        content.push_str("… stopped after 300 matches. Narrow the query or raise max_matches.\n");

        let compacted = format_tool_output_transform(
            "grep",
            Some(&call),
            &content,
            240,
            ToolOutputTransform::Compaction,
        );

        assert!(compacted.contains("Tool: `grep`"));
        assert!(compacted.contains("stopped after 300 matches"));
        assert!(compacted.contains("narrower query/path"));
        assert!(compacted.contains("read` around specific matching lines"));
    }

    #[test]
    fn compacted_shell_output_mentions_command_and_narrowing() {
        let call = named_call(
            "call_1",
            "shell",
            json!({"command":"cargo test --locked --all-targets"}),
        );
        let content = format!("stdout:\n{}\nexit code: 0\n", "test output\n".repeat(200));

        let compacted = format_tool_output_transform(
            "shell",
            Some(&call),
            &content,
            240,
            ToolOutputTransform::Compaction,
        );

        assert!(compacted.contains("Tool: `shell`"));
        assert!(compacted.contains("Shell command: `cargo test --locked --all-targets`"));
        assert!(compacted.contains("filter output with grep/head/tail"));
        assert!(compacted.contains("before making edits from omitted lines"));
    }

    #[test]
    fn model_tool_result_limit_is_model_facing_only() {
        let original = numbered_read_output("src/main.rs", 1, 120);
        let records = vec![
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call_with_lines("call_1", "src/main.rs", "1-120")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_1".into(),
                name: "read".into(),
                ok: true,
                content: original.clone(),
                ts: now_ts(),
            },
        ];
        let mut config = Config::default();
        config.model_tool_result_limit = 180;

        let messages = build_messages(&records, "system".into(), &config);

        assert_valid_tool_structure(&messages);
        assert!(messages.iter().any(|message| matches!(
            message,
            ModelMessage::Tool { content, .. }
                if content.contains("Cass truncated this tool output")
                    && content.contains("src/main.rs lines 1-120")
        )));
        assert!(matches!(
            &records[1],
            Record::Tool { content, .. } if content == &original
        ));
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
    fn repeated_read_of_same_range_supersedes_old_output() {
        let records = vec![
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
                content: "--- /tmp/example.rs lines 1-2 ---\n     1 | old one\n     2 | old two\n"
                    .into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call("call_2")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_2".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 1-2 ---\n     1 | new one\n     2 | new two\n"
                    .into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &Config::default());
        assert_valid_tool_structure(&messages);

        let first = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_1"))
            .unwrap();
        assert!(matches!(
            first,
            ModelMessage::Tool { content, .. }
                if content.contains("Cass omitted this earlier read output")
                    && !content.contains("old one")
        ));
        let second = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_2"))
            .unwrap();
        assert!(matches!(
            second,
            ModelMessage::Tool { content, .. } if content.contains("new one")
        ));
    }

    #[test]
    fn later_full_read_supersedes_old_output_even_when_file_shrinks() {
        let records = vec![
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
                content: "--- /tmp/example.rs lines 1-3 ---\n     1 | old one\n     2 | old two\n     3 | deleted\n".into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call("call_2")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_2".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 1-2 ---\n     1 | new one\n     2 | new two\n".into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &Config::default());

        let first = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_1"))
            .unwrap();
        assert!(matches!(
            first,
            ModelMessage::Tool { content, .. }
                if content.contains("Cass omitted this earlier read output")
                    && !content.contains("deleted")
        ));
    }

    #[test]
    fn later_partial_read_of_different_range_does_not_supersede_old_output() {
        let records = vec![
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call_with_lines("call_1", "src/main.rs", "1-2")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_1".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 1-2 ---\n     1 | keep one\n     2 | keep two\n".into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call_with_lines("call_2", "src/main.rs", "10-12")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_2".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 10-12 ---\n    10 | other\n    11 | lines\n    12 | here\n".into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &Config::default());

        let first = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_1"))
            .unwrap();
        assert!(matches!(
            first,
            ModelMessage::Tool { content, .. }
                if content.contains("keep one")
                    && !content.contains("Cass omitted this earlier read output")
        ));
    }

    #[test]
    fn later_wider_read_supersedes_covered_partial_output() {
        let records = vec![
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call_with_lines("call_1", "src/main.rs", "10-12")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_1".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 10-12 ---\n    10 | old\n    11 | old\n    12 | old\n".into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call_with_lines("call_2", "src/main.rs", "1-20")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_2".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/example.rs lines 1-20 ---\n     1 | new\n    10 | new\n    20 | new\n".into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &Config::default());

        let first = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_1"))
            .unwrap();
        assert!(matches!(
            first,
            ModelMessage::Tool { content, .. }
                if content.contains("Cass omitted this earlier read output")
                    && !content.contains("old")
        ));
    }

    #[test]
    fn multi_file_read_only_replaces_superseded_sections() {
        let records = vec![
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
                content: "--- /tmp/a.rs lines 1-1 ---\n     1 | stale\n--- /tmp/b.rs lines 1-1 ---\n     1 | keep\n".into(),
                ts: now_ts(),
            },
            Record::Assistant {
                content: String::new(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![call("call_2")],
                ts: now_ts(),
            },
            Record::Tool {
                tool_call_id: "call_2".into(),
                name: "read".into(),
                ok: true,
                content: "--- /tmp/a.rs lines 1-1 ---\n     1 | fresh\n".into(),
                ts: now_ts(),
            },
        ];
        let messages = build_messages(&records, "system".into(), &Config::default());

        let first = messages
            .iter()
            .find(|message| matches!(message, ModelMessage::Tool { tool_call_id, .. } if tool_call_id == "call_1"))
            .unwrap();
        assert!(matches!(
            first,
            ModelMessage::Tool { content, .. }
                if content.contains("Cass omitted this earlier read output")
                    && !content.contains("stale")
                    && content.contains("keep")
        ));
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
