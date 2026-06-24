use super::{schema, ToolContext, ToolSpec};
use anyhow::{bail, Result};
use ignore::WalkBuilder;
use regex::RegexBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Args {
    query: String,
    #[serde(default = "default_paths")]
    paths: Vec<String>,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    context_lines: usize,
    #[serde(default = "default_max")]
    max_matches: usize,
}
fn default_paths() -> Vec<String> {
    vec![".".into()]
}
fn default_max() -> usize {
    100
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "grep".into(),
        description: "Search files or directories for literal text or regex matches. Use before read for large inputs. In read-only and workspace-edit modes, paths must stay inside the launch cwd or bundled docs directory.".into(),
        parameters: schema::object(json!({
            "query": {"type":"string"},
            "paths": {"type":"array", "items":{"type":"string"}, "default":["."]},
            "regex": {"type":"boolean", "default":false},
            "case_sensitive": {"type":"boolean", "default":false},
            "context_lines": {"type":"integer", "default":0, "minimum":0, "maximum":5},
            "max_matches": {"type":"integer", "default":100, "minimum":1, "maximum":1000}
        }), &["query"]),
    }
}

pub fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    if args.query.is_empty() {
        bail!("grep query must not be empty");
    }
    let max_matches = args.max_matches.clamp(1, 1000);
    let context = args.context_lines.min(5);
    let re = if args.regex {
        RegexBuilder::new(&args.query)
            .case_insensitive(!args.case_sensitive)
            .build()?
    } else {
        RegexBuilder::new(&regex::escape(&args.query))
            .case_insensitive(!args.case_sensitive)
            .build()?
    };

    let mut files = Vec::new();
    for p in &args.paths {
        let path = super::path::resolve_existing(p, &ctx.cwd, &ctx.read_roots, ctx.mode)?;
        collect_files(&path, &mut files)?;
    }

    let mut out = String::new();
    let mut count = 0usize;
    'files: for file in files {
        let bytes = match fs::read(&file) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if super::path::is_probably_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                count += 1;
                if context == 0 {
                    out.push_str(&format!("{}:{}: {}\n", file.display(), idx + 1, line));
                } else {
                    out.push_str(&format!("--- {}:{} ---\n", file.display(), idx + 1));
                    let start = idx.saturating_sub(context);
                    let end = (idx + context + 1).min(lines.len());
                    for (j, l) in lines.iter().enumerate().take(end).skip(start) {
                        let mark = if j == idx { ">" } else { " " };
                        out.push_str(&format!("{mark}{:>6} | {}\n", j + 1, l));
                    }
                }
                if count >= max_matches {
                    out.push_str(&format!(
                        "… stopped after {count} matches. Narrow the query or raise max_matches.\n"
                    ));
                    break 'files;
                }
            }
        }
    }
    if count == 0 {
        out.push_str("no matches\n");
    }
    Ok(out)
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        bail!("not a file or directory: {}", path.display());
    }
    let mut builder = WalkBuilder::new(path);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true);
    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let p = entry.path();
        if should_skip(p) {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                // ignore crate cannot prune here without filter_entry; cheap enough for v1.
            }
            continue;
        }
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            files.push(p.to_path_buf());
        }
    }
    Ok(())
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        matches!(
            s.as_ref(),
            ".git" | "target" | "node_modules" | "vendor" | ".cache"
        )
    })
}
