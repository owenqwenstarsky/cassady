use super::{schema, ToolContext, ToolSpec};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;

#[derive(Debug, Deserialize)]
struct Args {
    files: Vec<FileArg>,
}

#[derive(Debug, Deserialize)]
struct FileArg {
    path: String,
    lines: Option<String>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "read".into(),
        description: "Read one or more text files, optionally with 1-indexed line ranges like 35-60, 35-, or -60. In read-only and workspace-edit modes, paths must stay inside the launch cwd or bundled docs directory.".into(),
        parameters: schema::object(json!({
            "files": {
                "type":"array",
                "items": {
                    "type":"object",
                    "properties": {
                        "path": {"type":"string"},
                        "lines": {"type":"string", "description":"Optional line range, e.g. 35-60, 35-, -60"}
                    },
                    "required":["path"],
                    "additionalProperties": false
                }
            }
        }), &["files"]),
    }
}

pub fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    if args.files.is_empty() {
        bail!("read requires at least one file");
    }
    let mut out = String::new();
    for file in args.files {
        let path = super::path::resolve_existing(&file.path, &ctx.cwd, &ctx.read_roots, ctx.mode)?;
        if !path.is_file() {
            bail!("not a file: {}", path.display());
        }
        let bytes = fs::read(&path)?;
        if super::path::is_probably_binary(&bytes) {
            bail!("binary files are unsupported: {}", path.display());
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let (start, end) = parse_range(file.lines.as_deref(), lines.len())?;
        out.push_str(&format!(
            "--- {} lines {}-{} ---\n",
            path.display(),
            start,
            end
        ));
        for (idx, line) in lines
            .iter()
            .enumerate()
            .take(end)
            .skip(start.saturating_sub(1))
        {
            out.push_str(&format!("{:>6} | {}\n", idx + 1, line));
        }
    }
    Ok(out)
}

fn parse_range(range: Option<&str>, len: usize) -> Result<(usize, usize)> {
    if len == 0 {
        return Ok((1, 0));
    }
    let Some(range) = range.filter(|s| !s.trim().is_empty()) else {
        return Ok((1, len));
    };
    let parts: Vec<_> = range.split('-').collect();
    if parts.len() != 2 {
        bail!("invalid line range `{range}`");
    }
    let start = if parts[0].is_empty() {
        1
    } else {
        parts[0].parse::<usize>()?
    };
    let end = if parts[1].is_empty() {
        len
    } else {
        parts[1].parse::<usize>()?
    };
    if start == 0 || end < start {
        bail!("invalid line range `{range}`");
    }
    Ok((start.min(len), end.min(len)))
}
