use super::types::{CompletionResult, ModelMessage};
use crate::agent::AgentEvent;
use crate::conversation::StoredToolCall;
use crate::tools::ToolSpec;
use anyhow::{bail, Context, Result};
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
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(model: String, base_url: String, api_key_env: String) -> Result<Self> {
        let api_key = std::env::var(&api_key_env)
            .with_context(|| format!("missing API key environment variable `{api_key_env}`"))?;
        Ok(Self {
            client: Client::new(),
            model,
            base_url: normalize_base_url(&base_url),
            api_key,
        })
    }

    pub async fn complete(
        &self,
        messages: Vec<ModelMessage>,
        tools: Vec<ToolSpec>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<CompletionResult> {
        let url = chat_url(&self.base_url);
        let body = json!({
            "model": self.model,
            "messages": messages_to_openai(messages),
            "tools": tools_to_openai(tools),
            "stream": true
        });
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
        let mut partials: BTreeMap<usize, PartialToolCall> = BTreeMap::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find("\n\n") {
                let frame = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();
                for line in frame.lines() {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        continue;
                    }
                    handle_chunk(data, &mut content, &mut partials, tx)?;
                }
            }
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
            tool_calls,
        })
    }
}

fn handle_chunk(
    data: &str,
    content: &mut String,
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

fn messages_to_openai(messages: Vec<ModelMessage>) -> Vec<Value> {
    messages.into_iter().map(|m| match m {
        ModelMessage::System { content } => json!({"role":"system", "content":content}),
        ModelMessage::User { content } => json!({"role":"user", "content":content}),
        ModelMessage::Assistant { content, tool_calls } => {
            if tool_calls.is_empty() {
                json!({"role":"assistant", "content":content})
            } else {
                let calls: Vec<_> = tool_calls.into_iter().map(|tc| json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {"name": tc.name, "arguments": tc.arguments.to_string()}
                })).collect();
                json!({"role":"assistant", "content": if content.is_empty() { Value::Null } else { Value::String(content) }, "tool_calls": calls})
            }
        }
        ModelMessage::Tool { tool_call_id, name: _, content } => json!({"role":"tool", "tool_call_id":tool_call_id, "content":content}),
    }).collect()
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
