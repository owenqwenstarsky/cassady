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
        description: "Safely edit a file using exact old_text/new_text replacements. Every old_text must match exactly once in the original file. Requires full-access mode.".into(),
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
    let path = super::path::resolve_for_write(&args.path, &ctx.cwd, ctx.mode)?;
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
        "applied {} edit(s) to {}",
        args.edits.len(),
        path.display()
    ))
}

fn preview(s: &str) -> String {
    s.chars().take(80).collect()
}
