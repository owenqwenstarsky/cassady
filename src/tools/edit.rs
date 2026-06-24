use super::{schema, ToolContext, ToolSpec};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::ops::Range;

#[derive(Debug, Deserialize)]
struct Args {
    path: String,
    edits: Vec<EditArg>,
}
#[derive(Debug, Deserialize)]
struct EditArg {
    old_text: String,
    new_text: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "edit".into(),
        description: "Safely edit a file using exact old_text/new_text replacements when the active access policy permits writes. Every old_text must match exactly once in the original file. In workspace-edit mode, edits must stay inside the launch workspace. Writes under Cass bundled docs are blocked.".into(),
        parameters: schema::object(json!({
            "path": {"type":"string"},
            "edits": {"type":"array", "items": {
                "type":"object",
                "properties": {
                    "old_text": {"type":"string"},
                    "new_text": {"type":"string"}
                },
                "required":["old_text", "new_text"],
                "additionalProperties": false
            }}
        }), &["path", "edits"]),
    }
}

pub fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    if args.edits.is_empty() {
        bail!("edit requires at least one replacement");
    }
    let path =
        super::path::resolve_for_write(&args.path, &ctx.cwd, ctx.mode, &ctx.blocked_write_roots)?;
    let original = fs::read_to_string(&path)?;
    let mut ranges: Vec<(Range<usize>, &str)> = Vec::new();
    for edit in &args.edits {
        if edit.old_text.is_empty() {
            bail!("old_text must not be empty");
        }
        let matches: Vec<_> = original.match_indices(&edit.old_text).collect();
        if matches.is_empty() {
            bail!("old_text not found: {:?}", preview(&edit.old_text));
        }
        if matches.len() > 1 {
            bail!("old_text is not unique: {:?}", preview(&edit.old_text));
        }
        let start = matches[0].0;
        ranges.push((start..start + edit.old_text.len(), edit.new_text.as_str()));
    }
    ranges.sort_by_key(|(r, _)| r.start);
    for pair in ranges.windows(2) {
        if pair[0].0.end > pair[1].0.start {
            bail!("edits overlap; no changes written");
        }
    }
    let mut out = String::new();
    let mut cursor = 0;
    for (range, replacement) in &ranges {
        out.push_str(&original[cursor..range.start]);
        out.push_str(replacement);
        cursor = range.end;
    }
    out.push_str(&original[cursor..]);
    super::write::atomic_write(&path, out.as_bytes())?;
    Ok(format!(
        "applied {} edit(s) to {}\n\n{}",
        args.edits.len(),
        path.display(),
        unified_diff(&path.display().to_string(), &original, &out)
    ))
}

fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut out = String::new();
    out.push_str(&format!("--- {path} before\n"));
    out.push_str(&format!("+++ {path} after\n"));
    out.push_str("@@\n");
    let max = before_lines.len().max(after_lines.len());
    for idx in 0..max {
        match (before_lines.get(idx), after_lines.get(idx)) {
            (Some(a), Some(b)) if a == b => out.push_str(&format!(" {a}\n")),
            (Some(a), Some(b)) => {
                out.push_str(&format!("-{a}\n"));
                out.push_str(&format!("+{b}\n"));
            }
            (Some(a), None) => out.push_str(&format!("-{a}\n")),
            (None, Some(b)) => out.push_str(&format!("+{b}\n")),
            (None, None) => {}
        }
    }
    out
}

fn preview(s: &str) -> String {
    s.chars().take(80).collect()
}
