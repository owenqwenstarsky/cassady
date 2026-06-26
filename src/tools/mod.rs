pub mod edit;
pub mod grep;
pub mod ls;
pub mod path;
pub mod read;
pub mod schema;
pub mod shell;
pub mod write;

use crate::access::AccessMode;
use crate::security::{
    PolicyDecision, SecurityContext, SecurityPolicy, ToolAction, ToolAvailability,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub mode: AccessMode,
    pub cwd: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub blocked_write_roots: Vec<PathBuf>,
    pub model_result_limit: usize,
    pub runtime_tx: Option<mpsc::UnboundedSender<ToolRuntimeEvent>>,
}

#[derive(Debug, Clone)]
pub enum ToolRuntimeEvent {
    OutputChunk { stream: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub ok: bool,
    pub content: String,
}

impl ToolContext {
    pub fn security_context(&self) -> SecurityContext {
        SecurityContext {
            mode: self.mode,
            cwd: self.cwd.clone(),
            read_roots: self.read_roots.clone(),
            blocked_write_roots: self.blocked_write_roots.clone(),
        }
    }
}

pub fn available_tool_names(mode: AccessMode) -> Vec<String> {
    let ctx = SecurityContext {
        mode,
        cwd: PathBuf::new(),
        read_roots: Vec::new(),
        blocked_write_roots: Vec::new(),
    };
    all_tool_names()
        .into_iter()
        .filter(|name| {
            SecurityPolicy::tool_availability(&ctx, name) != ToolAvailability::Unavailable
        })
        .map(str::to_string)
        .collect()
}

pub fn specs(mode: AccessMode) -> Vec<ToolSpec> {
    let ctx = SecurityContext {
        mode,
        cwd: PathBuf::new(),
        read_roots: Vec::new(),
        blocked_write_roots: Vec::new(),
    };
    specs_for_context(&ctx)
}

pub fn specs_for_context(ctx: &SecurityContext) -> Vec<ToolSpec> {
    let mut out = Vec::new();
    for name in all_tool_names() {
        if SecurityPolicy::tool_availability(ctx, name) == ToolAvailability::Unavailable {
            continue;
        }
        out.push(match name {
            "ls" => ls::spec(),
            "read" => read::spec(),
            "grep" => grep::spec(),
            "write" => write::spec(),
            "edit" => edit::spec(),
            "shell" => shell::spec(),
            _ => continue,
        });
    }
    out
}

fn all_tool_names() -> [&'static str; 6] {
    ["ls", "read", "grep", "write", "edit", "shell"]
}

pub async fn execute(name: &str, args: Value, ctx: &ToolContext) -> ToolOutput {
    execute_with_approval(name, args, ctx, false).await
}

pub async fn execute_with_approval(
    name: &str,
    args: Value,
    ctx: &ToolContext,
    approved: bool,
) -> ToolOutput {
    if let Some(decision) = policy_decision_for_call(name, &args, ctx) {
        match decision {
            PolicyDecision::Allow => {}
            PolicyDecision::Ask { reason: _ } if approved => {}
            PolicyDecision::Ask { reason } => {
                return ToolOutput {
                    ok: false,
                    content: format!("approval required before executing `{name}`: {reason}"),
                };
            }
            PolicyDecision::Deny { reason } => {
                return ToolOutput {
                    ok: false,
                    content: reason,
                };
            }
        }
    }

    let result: Result<String> = match name {
        "shell" => shell::run(args, ctx).await,
        "ls" => ls::run(args, ctx),
        "read" => read::run(args, ctx),
        "grep" => grep::run(args, ctx),
        "write" => write::run(args, ctx),
        "edit" => edit::run(args, ctx),
        _ => Err(anyhow::anyhow!("unknown tool `{name}`")),
    };
    result_to_output(result, ctx)
}

pub fn policy_decision_for_call(
    name: &str,
    args: &Value,
    ctx: &ToolContext,
) -> Option<PolicyDecision> {
    Some(SecurityPolicy::check(
        &ctx.security_context(),
        &tool_action(name, args, ctx)?,
    ))
}

fn tool_action(name: &str, args: &Value, ctx: &ToolContext) -> Option<ToolAction> {
    let resolve = |path: &str| {
        let p = path::expand_tilde(path);
        if p.is_absolute() {
            p
        } else {
            ctx.cwd.join(p)
        }
    };
    match name {
        "ls" => Some(ToolAction::List {
            path: resolve(args.get("path").and_then(Value::as_str).unwrap_or(".")),
        }),
        "read" => args
            .get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.get("path"))
            .and_then(Value::as_str)
            .map(|path| ToolAction::Read {
                path: resolve(path),
            }),
        "grep" => args
            .get("paths")
            .and_then(Value::as_array)
            .and_then(|paths| paths.first())
            .and_then(Value::as_str)
            .or(Some("."))
            .map(|path| ToolAction::Search {
                path: resolve(path),
            }),
        "write" => args
            .get("path")
            .and_then(Value::as_str)
            .map(|path| ToolAction::Write {
                path: resolve(path),
            }),
        "edit" => args
            .get("path")
            .and_then(Value::as_str)
            .map(|path| ToolAction::Edit {
                path: resolve(path),
            }),
        "shell" => args
            .get("command")
            .and_then(Value::as_str)
            .map(|command| ToolAction::Shell {
                command: command.to_string(),
            }),
        _ => None,
    }
}

fn result_to_output(result: Result<String>, ctx: &ToolContext) -> ToolOutput {
    match result {
        Ok(content) => ToolOutput {
            ok: true,
            content: truncate_model(content, ctx.model_result_limit),
        },
        Err(err) => ToolOutput {
            ok: false,
            content: err.to_string(),
        },
    }
}

fn truncate_model(mut s: String, limit: usize) -> String {
    if s.len() <= limit {
        return s;
    }
    let mut end = limit.min(s.len());
    while !s.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    s.truncate(end);
    s.push_str("\n… truncated by Cass; use grep or narrower line ranges for more.");
    s
}
