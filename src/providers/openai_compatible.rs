use super::types::{CompletionResult, ModelMessage};
use crate::agent::AgentEvent;
use crate::config::{ReasoningEffort, ReasoningRequestFormat};
use crate::conversation::StoredToolCall;
use crate::tools::ToolSpec;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    client: Client,
    model: String,
    base_url: String,
    api_key: String,
    reasoning_effort: ReasoningEffort,
    reasoning_request_format: ReasoningRequestFormat,
    reasoning_supported: bool,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleSettings {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_request_format: ReasoningRequestFormat,
    pub reasoning_supported: bool,
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(settings: OpenAiCompatibleSettings) -> Self {
        Self {
            client: Client::new(),
            model: settings.model,
            base_url: normalize_base_url(&settings.base_url),
            api_key: settings.api_key,
            reasoning_effort: settings.reasoning_effort,
            reasoning_request_format: settings.reasoning_request_format,
            reasoning_supported: settings.reasoning_supported,
        }
    }

    pub async fn complete(
        &self,
        messages: Vec<ModelMessage>,
        tools: Vec<ToolSpec>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<CompletionResult> {
        let url = chat_url(&self.base_url);
        let mut body = json!({
            "model": self.model,
            "messages": messages_to_openai(messages),
            "tools": tools_to_openai(tools),
            "stream": true
        });
        apply_reasoning_request(
            &mut body,
            self.reasoning_effort,
            self.reasoning_request_format,
            self.reasoning_supported,
        );
        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("provider returned {status}: {text}");
        }

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut reasoning_field: Option<String> = None;
        let mut partials: BTreeMap<usize, PartialToolCall> = BTreeMap::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let chunk_text = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            buf.push_str(&chunk_text);
            while let Some(pos) = buf.find("\n\n") {
                let frame = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();
                process_frame(
                    &frame,
                    &mut content,
                    &mut reasoning,
                    &mut reasoning_field,
                    &mut partials,
                    tx,
                )?;
            }
        }
        if !buf.trim().is_empty() {
            process_frame(
                &buf,
                &mut content,
                &mut reasoning,
                &mut reasoning_field,
                &mut partials,
                tx,
            )?;
        }

        let tool_calls = partials
            .into_iter()
            .filter_map(|(idx, p)| {
                let name = p.name?;
                let id = p.id.unwrap_or_else(|| format!("call_{idx}"));
                let arguments = serde_json::from_str(&p.arguments)
                    .unwrap_or_else(|_| json!({"_raw": p.arguments}));
                Some(StoredToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect();
        Ok(CompletionResult {
            content,
            reasoning,
            reasoning_field,
            tool_calls,
        })
    }
}

fn process_frame(
    frame: &str,
    content: &mut String,
    reasoning: &mut String,
    reasoning_field: &mut Option<String>,
    partials: &mut BTreeMap<usize, PartialToolCall>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> Result<()> {
    for line in frame.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let data = line.trim_start_matches("data:").trim();
        if data == "[DONE]" {
            continue;
        }
        handle_chunk(data, content, reasoning, reasoning_field, partials, tx)?;
    }
    Ok(())
}

fn handle_chunk(
    data: &str,
    content: &mut String,
    reasoning: &mut String,
    reasoning_field: &mut Option<String>,
    partials: &mut BTreeMap<usize, PartialToolCall>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> Result<()> {
    let v: Value = serde_json::from_str(data)?;
    let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
        return Ok(());
    };
    let delta = choice
        .get("delta")
        .or_else(|| choice.get("message"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some((field, s)) = reasoning_delta(&delta) {
        reasoning_field.get_or_insert_with(|| field.to_string());
        reasoning.push_str(s);
        let _ = tx.send(AgentEvent::ReasoningChunk(s.to_string()));
    }
    if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
        content.push_str(s);
        let _ = tx.send(AgentEvent::AssistantChunk(s.to_string()));
    }
    if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let idx = call.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let p = partials.entry(idx).or_default();
            if let Some(id) = call.get("id").and_then(|i| i.as_str()) {
                p.id = Some(id.to_string());
            }
            if let Some(function) = call.get("function") {
                if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                    p.name = Some(name.to_string());
                }
                if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                    p.arguments.push_str(args);
                }
            }
        }
    }
    Ok(())
}

fn reasoning_delta(delta: &Value) -> Option<(&'static str, &str)> {
    ["reasoning_content", "reasoning", "thinking", "thought"]
        .into_iter()
        .find_map(|field| {
            delta
                .get(field)
                .and_then(|v| v.as_str())
                .map(|s| (field, s))
        })
}

fn apply_reasoning_request(
    body: &mut Value,
    effort: ReasoningEffort,
    format: ReasoningRequestFormat,
    supported: bool,
) {
    if !supported {
        return;
    }
    let effort_str = match effort {
        ReasoningEffort::Off => "none",
        _ => match effort.request_value() {
            Some(value) => value,
            None => return,
        },
    };
    let Value::Object(obj) = body else {
        return;
    };
    match format {
        ReasoningRequestFormat::ReasoningEffort => {
            obj.insert(
                "reasoning_effort".to_string(),
                Value::String(effort_str.to_string()),
            );
        }
        ReasoningRequestFormat::ReasoningObject => {
            obj.insert("reasoning".to_string(), json!({ "effort": effort_str }));
        }
    }
}

fn assistant_message_to_openai(
    content: String,
    reasoning: String,
    reasoning_field: Option<String>,
    tool_calls: Vec<StoredToolCall>,
) -> Value {
    let mut message = json!({"role":"assistant", "content":content});
    if let Value::Object(ref mut obj) = message {
        if !reasoning.trim().is_empty() {
            let field = reasoning_field.unwrap_or_else(|| "reasoning_content".to_string());
            obj.insert(field, Value::String(reasoning));
        }
        if !tool_calls.is_empty() {
            let calls: Vec<_> = tool_calls
                .into_iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {"name": tc.name, "arguments": tc.arguments.to_string()}
                    })
                })
                .collect();
            if obj
                .get("content")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.is_empty())
            {
                obj.insert("content".to_string(), Value::Null);
            }
            obj.insert("tool_calls".to_string(), Value::Array(calls));
        }
    }
    message
}

fn messages_to_openai(messages: Vec<ModelMessage>) -> Vec<Value> {
    messages
        .into_iter()
        .map(|m| match m {
            ModelMessage::System { content } => json!({"role":"system", "content":content}),
            ModelMessage::User { content } => json!({"role":"user", "content":content}),
            ModelMessage::Assistant {
                content,
                reasoning,
                reasoning_field,
                tool_calls,
            } => assistant_message_to_openai(content, reasoning, reasoning_field, tool_calls),
            ModelMessage::Tool {
                tool_call_id,
                name: _,
                content,
            } => json!({"role":"tool", "tool_call_id":tool_call_id, "content":content}),
        })
        .collect()
}

fn tools_to_openai(tools: Vec<ToolSpec>) -> Vec<Value> {
    tools.into_iter().map(|t| json!({
        "type": "function",
        "function": {"name": t.name, "description": t.description, "parameters": t.parameters}
    })).collect()
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .to_string()
}

fn chat_url(base: &str) -> String {
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{}/chat/completions", base.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_effort_format_sends_none_when_off_and_supported() {
        let mut body = json!({"model": "test"});
        apply_reasoning_request(
            &mut body,
            ReasoningEffort::Off,
            ReasoningRequestFormat::ReasoningEffort,
            true,
        );
        assert_eq!(
            body["reasoning_effort"],
            Value::String("none".to_string())
        );
    }

    #[test]
    fn reasoning_object_format_sends_none_when_off_and_supported() {
        let mut body = json!({"model": "test"});
        apply_reasoning_request(
            &mut body,
            ReasoningEffort::Off,
            ReasoningRequestFormat::ReasoningObject,
            true,
        );
        assert_eq!(body["reasoning"], json!({ "effort": "none" }));
    }

    #[test]
    fn reasoning_sends_nothing_when_unsupported_even_if_off() {
        let mut body = json!({"model": "test"});
        apply_reasoning_request(
            &mut body,
            ReasoningEffort::Off,
            ReasoningRequestFormat::ReasoningEffort,
            false,
        );
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn reasoning_sends_nothing_when_unsupported_even_if_high() {
        let mut body = json!({"model": "test"});
        apply_reasoning_request(
            &mut body,
            ReasoningEffort::High,
            ReasoningRequestFormat::ReasoningObject,
            false,
        );
        assert!(body.get("reasoning").is_none());
    }
}
