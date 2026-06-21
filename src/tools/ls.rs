use super::{schema, ToolContext, ToolSpec};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;

#[derive(Debug, Deserialize)]
struct Args {
    #[serde(default = "default_path")]
    path: String,
}
fn default_path() -> String {
    ".".into()
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "ls".into(),
        description: "List a directory. In read-only mode, paths must stay inside the launch cwd or bundled docs directory."
            .into(),
        parameters: schema::object(
            json!({
                "path": {"type":"string", "description":"Directory path to list", "default":"."}
            }),
            &["path"],
        ),
    }
}

pub fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    let path = super::path::resolve_existing(&args.path, &ctx.cwd, &ctx.read_roots, ctx.mode)?;
    if !path.is_dir() {
        bail!("not a directory: {}", path.display());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let mut name = entry.file_name().to_string_lossy().to_string();
        let kind = if meta.is_dir() {
            name.push('/');
            "dir".to_string()
        } else {
            format_size(meta.len())
        };
        entries.push((name, kind));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let name_width = entries
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(4)
        .min(48);
    let mut out = format!("{}\n", path.display());
    for (name, kind) in entries {
        out.push_str(&format!("  {name:<name_width$}  {kind}\n"));
    }
    Ok(out)
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else if size >= 10.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
