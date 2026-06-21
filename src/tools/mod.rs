pub mod edit;
pub mod grep;
pub mod ls;
pub mod path;
pub mod read;
pub mod schema;
pub mod write;

use crate::access::AccessMode;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

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
    pub read_only_root: PathBuf,
    pub model_result_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub ok: bool,
    pub content: String,
}

pub fn available_tool_names(mode: AccessMode) -> Vec<String> {
    match mode {
        AccessMode::ReadOnly => vec!["ls".into(), "read".into(), "grep".into()],
        AccessMode::FullAccess => vec![
            "ls".into(),
            "read".into(),
            "grep".into(),
            "write".into(),
            "edit".into(),
        ],
    }
}

pub fn specs(mode: AccessMode) -> Vec<ToolSpec> {
    let mut specs = vec![ls::spec(), read::spec(), grep::spec()];
    if mode.can_write() {
        specs.push(write::spec());
        specs.push(edit::spec());
    }
    specs
}

pub async fn execute(name: &str, args: Value, ctx: &ToolContext) -> ToolOutput {
    let result: Result<String> = match name {
        "ls" => ls::run(args, ctx),
        "read" => read::run(args, ctx),
        "grep" => grep::run(args, ctx),
        "write" if ctx.mode.can_write() => write::run(args, ctx),
        "edit" if ctx.mode.can_write() => edit::run(args, ctx),
        "write" | "edit" => Err(anyhow::anyhow!(
            "tool `{name}` is unavailable in {} mode",
            ctx.mode
        )),
        _ => Err(anyhow::anyhow!("unknown tool `{name}`")),
    };
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
    s.truncate(limit);
    s.push_str("\n… truncated by Cass; use grep or narrower line ranges for more.");
    s
}
