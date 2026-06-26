use super::types::{CompletionResult, ModelMessage};
use crate::agent::AgentEvent;
use crate::codex_auth::load_codex_access_token;
use crate::config::{ReasoningEffort, CHATGPT_CODEX_DEFAULT_MODEL, CHATGPT_CODEX_RESPONSES_URL};
use crate::conversation::StoredToolCall;
use crate::tools::ToolSpec;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ChatGptCodexProvider {
    client: Client,
    model: String,
    endpoint: String,
    reasoning_effort: ReasoningEffort,
    fast_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ChatGptCodexSettings {
    pub model: String,
    pub endpoint: String,
    pub reasoning_effort: ReasoningEffort,
    pub fast_mode: bool,
}

#[derive(Debug, Default, Clone)]
struct PartialFunctionCall {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ChatGptCodexProvider {
    pub fn new(settings: ChatGptCodexSettings) -> Self {
        Self {
            client: Client::new(),
            model: settings.model,
            endpoint: normalize_endpoint(&settings.endpoint),
            reasoning_effort: settings.reasoning_effort,
            fast_mode: settings.fast_mode,
        }
    }

    pub async fn complete(
        &self,
        messages: Vec<ModelMessage>,
        tools: Vec<ToolSpec>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<CompletionResult> {
        let token = load_codex_access_token()?;
        let secret = token.as_secret().to_string();
        let body = responses_body(
            &self.model,
            messages,
            tools,
            self.reasoning_effort,
            self.fast_mode,
        );
        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token.as_secret())
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!(
                "ChatGPT Codex returned {status}: {}",
                redact_secret(&text, &secret)
            );
        }

        let mut state = StreamState::default();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let chunk_text = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            buf.push_str(&chunk_text);
            while let Some(pos) = buf.find("\n\n") {
                let frame = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();
                process_frame(&frame, &mut state, tx)?;
            }
        }
        if !buf.trim().is_empty() {
            process_frame(&buf, &mut state, tx)?;
        }

        Ok(state.finish())
    }
}

#[derive(Debug, Default)]
struct StreamState {
    content: String,
    reasoning: String,
    reasoning_field: Option<String>,
    partials: BTreeMap<String, PartialFunctionCall>,
}

impl StreamState {
    fn finish(self) -> CompletionResult {
        let tool_calls = self
            .partials
            .into_iter()
            .filter_map(|(key, partial)| {
                let name = partial.name?;
                let id = partial.call_id.unwrap_or(key);
                let arguments = serde_json::from_str(&partial.arguments)
                    .unwrap_or_else(|_| json!({"_raw": partial.arguments}));
                Some(StoredToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect();
        CompletionResult {
            content: self.content,
            reasoning: self.reasoning,
            reasoning_field: self.reasoning_field,
            tool_calls,
        }
    }
}

fn responses_body(
    model: &str,
    messages: Vec<ModelMessage>,
    tools: Vec<ToolSpec>,
    reasoning_effort: ReasoningEffort,
    fast_mode: bool,
) -> Value {
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for message in messages {
        match message {
            ModelMessage::System { content } => instructions.push(content),
            ModelMessage::User { content } => input.push(json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": content}]
            })),
            ModelMessage::Assistant {
                content,
                reasoning: _,
                reasoning_field: _,
                tool_calls,
            } => {
                if !content.is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": content}]
                    }));
                }
                for call in tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": call.arguments.to_string()
                    }));
                }
            }
            ModelMessage::Tool {
                tool_call_id,
                name: _,
                content,
            } => input.push(json!({
                "type": "function_call_output",
                "call_id": tool_call_id,
                "output": content
            })),
        }
    }

    let mut body = json!({
        "model": model,
        "input": input,
        "tools": tools_to_responses(tools),
        "stream": true,
        "store": false
    });
    if !instructions.is_empty() {
        body["instructions"] = Value::String(instructions.join("\n\n"));
    }
    if fast_mode {
        body["reasoning"] = json!({"effort": fast_mode_reasoning_effort(model), "summary": "auto"});
    } else if let Some(effort) = reasoning_effort.request_value() {
        body["reasoning"] = json!({"effort": effort, "summary": "auto"});
    } else if reasoning_effort == ReasoningEffort::Off {
        body["reasoning"] = json!({"effort": "none", "summary": "auto"});
    }
    body
}

fn fast_mode_reasoning_effort(model: &str) -> &'static str {
    if model == CHATGPT_CODEX_DEFAULT_MODEL {
        "low"
    } else {
        "minimal"
    }
}

fn tools_to_responses(tools: Vec<ToolSpec>) -> Vec<Value> {
    tools
        .into_iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters
            })
        })
        .collect()
}

fn process_frame(
    frame: &str,
    state: &mut StreamState,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> Result<()> {
    for line in frame.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let data = line.trim_start_matches("data:").trim();
        if data == "[DONE]" || data.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(data)?;
        handle_event(&value, state, tx)?;
    }
    Ok(())
}

fn handle_event(
    value: &Value,
    state: &mut StreamState,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> Result<()> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "response.output_text.delta" | "response.message.delta" | "output_text.delta" => {
            if let Some(delta) = string_field(value, &["delta", "text"]) {
                push_content(state, tx, delta);
            }
        }
        "response.reasoning_summary_text.delta"
        | "response.reasoning_text.delta"
        | "response.reasoning.delta"
        | "reasoning.delta" => {
            if let Some(delta) = string_field(value, &["delta", "text"]) {
                push_reasoning(state, tx, "reasoning_summary", delta);
            }
        }
        "response.function_call_arguments.delta" | "function_call_arguments.delta" => {
            let key = event_key(value);
            let partial = state.partials.entry(key).or_default();
            if let Some(delta) = string_field(value, &["delta", "arguments_delta"]) {
                partial.arguments.push_str(delta);
            }
        }
        "response.function_call_arguments.done" | "function_call_arguments.done" => {
            let key = event_key(value);
            let partial = state.partials.entry(key).or_default();
            if let Some(arguments) = string_field(value, &["arguments"]) {
                partial.arguments = arguments.to_string();
            }
            if let Some(call_id) = string_field(value, &["call_id", "id"]) {
                partial.call_id = Some(call_id.to_string());
            }
            if let Some(name) = string_field(value, &["name"]) {
                partial.name = Some(name.to_string());
            }
        }
        "response.output_item.added"
        | "response.output_item.done"
        | "output_item.added"
        | "output_item.done" => {
            if let Some(item) = value.get("item") {
                handle_item(item, state, tx, event_type.ends_with("done"));
            }
        }
        "response.completed" | "response.done" => {
            if state.content.is_empty() {
                if let Some(response) = value.get("response") {
                    extract_final_response(response, state, tx);
                }
            }
        }
        _ => {
            if let Some(item) = value.get("item") {
                handle_item(item, state, tx, false);
            } else if let Some(delta) = value
                .get("delta")
                .and_then(Value::as_str)
                .filter(|_| event_type.contains("output_text"))
            {
                push_content(state, tx, delta);
            }
        }
    }
    Ok(())
}

fn handle_item(
    item: &Value,
    state: &mut StreamState,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    final_item: bool,
) {
    match item.get("type").and_then(Value::as_str).unwrap_or_default() {
        "message" => {
            if final_item && state.content.is_empty() {
                for content in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if matches!(
                        content.get("type").and_then(Value::as_str),
                        Some("output_text")
                    ) {
                        if let Some(text) = content.get("text").and_then(Value::as_str) {
                            push_content(state, tx, text);
                        }
                    }
                }
            }
        }
        "reasoning" => {
            for summary in item
                .get("summary")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(text) = summary.get("text").and_then(Value::as_str) {
                    push_reasoning(state, tx, "reasoning_summary", text);
                }
            }
        }
        "function_call" => {
            let key = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .or_else(|| item.get("item_id"))
                .and_then(Value::as_str)
                .unwrap_or("call")
                .to_string();
            let partial = state.partials.entry(key.clone()).or_default();
            if let Some(call_id) = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
            {
                partial.call_id = Some(call_id.to_string());
            }
            if let Some(name) = item.get("name").and_then(Value::as_str) {
                partial.name = Some(name.to_string());
            }
            if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
                partial.arguments = arguments.to_string();
            }
        }
        _ => {}
    }
}

fn extract_final_response(
    response: &Value,
    state: &mut StreamState,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) {
    for item in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        handle_item(item, state, tx, true);
    }
}

fn push_content(state: &mut StreamState, tx: &mpsc::UnboundedSender<AgentEvent>, text: &str) {
    state.content.push_str(text);
    let _ = tx.send(AgentEvent::AssistantChunk(text.to_string()));
}

fn push_reasoning(
    state: &mut StreamState,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    field: &'static str,
    text: &str,
) {
    state
        .reasoning_field
        .get_or_insert_with(|| field.to_string());
    state.reasoning.push_str(text);
    let _ = tx.send(AgentEvent::ReasoningChunk(text.to_string()));
}

fn string_field<'a>(value: &'a Value, fields: &[&str]) -> Option<&'a str> {
    fields.iter().find_map(|field| value.get(*field)?.as_str())
}

fn event_key(value: &Value) -> String {
    string_field(
        value,
        &["call_id", "item_id", "output_item_id", "id", "output_index"],
    )
    .map(str::to_string)
    .or_else(|| {
        value
            .get("output_index")
            .and_then(Value::as_u64)
            .map(|n| n.to_string())
    })
    .unwrap_or_else(|| "call".to_string())
}

fn normalize_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        CHATGPT_CODEX_RESPONSES_URL.to_string()
    } else {
        endpoint.to_string()
    }
}

fn redact_secret(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn responses_body_uses_function_call_items() {
        let body = responses_body(
            "gpt-test",
            vec![
                ModelMessage::System {
                    content: "system".into(),
                },
                ModelMessage::User {
                    content: "hello".into(),
                },
                ModelMessage::Assistant {
                    content: String::new(),
                    reasoning: String::new(),
                    reasoning_field: None,
                    tool_calls: vec![StoredToolCall {
                        id: "call_1".into(),
                        name: "read".into(),
                        arguments: json!({"path":"README.md"}),
                    }],
                },
                ModelMessage::Tool {
                    tool_call_id: "call_1".into(),
                    name: "read".into(),
                    content: "ok".into(),
                },
            ],
            Vec::new(),
            ReasoningEffort::Off,
            false,
        );

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["instructions"], "system");
        assert!(body["input"].as_array().unwrap().iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
        }));
    }

    #[test]
    fn responses_body_uses_low_reasoning_for_gpt_5_5_fast_mode() {
        let body = responses_body(
            CHATGPT_CODEX_DEFAULT_MODEL,
            vec![ModelMessage::User {
                content: "hello".into(),
            }],
            Vec::new(),
            ReasoningEffort::High,
            true,
        );

        assert_eq!(
            body["reasoning"],
            json!({"effort": "low", "summary": "auto"})
        );
    }

    #[test]
    fn responses_body_keeps_minimal_reasoning_for_other_fast_mode_models() {
        let body = responses_body(
            "gpt-test",
            vec![ModelMessage::User {
                content: "hello".into(),
            }],
            Vec::new(),
            ReasoningEffort::High,
            true,
        );

        assert_eq!(
            body["reasoning"],
            json!({"effort": "minimal", "summary": "auto"})
        );
    }

    #[test]
    fn responses_body_sends_none_effort_when_reasoning_is_off() {
        let body = responses_body(
            "gpt-test",
            vec![ModelMessage::User {
                content: "hello".into(),
            }],
            Vec::new(),
            ReasoningEffort::Off,
            false,
        );

        assert_eq!(
            body["reasoning"],
            json!({"effort": "none", "summary": "auto"})
        );
    }

    #[test]
    fn stream_parser_collects_text_and_function_call() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut state = StreamState::default();
        process_frame(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}\n\n",
            &mut state,
            &tx,
        )
        .unwrap();
        let result = state.finish();

        assert_eq!(result.content, "hi");
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "call_1");
        assert_eq!(result.tool_calls[0].name, "read");
    }
}
