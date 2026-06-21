use super::{schema, ToolContext, ToolSpec};
use anyhow::{bail, Result};
use nanoid::nanoid;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::Write as _;

#[derive(Debug, Deserialize)]
struct Args {
    path: String,
    content: String,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "write".into(),
        description: "Create or overwrite a text file. Requires full-access mode. Writes under Cass bundled docs are blocked. Uses atomic temp-file-and-rename where practical.".into(),
        parameters: schema::object(json!({
            "path": {"type":"string"},
            "content": {"type":"string"}
        }), &["path", "content"]),
    }
}

pub fn run(args: Value, ctx: &ToolContext) -> Result<String> {
    let args: Args = serde_json::from_value(args)?;
    let path =
        super::path::resolve_for_write(&args.path, &ctx.cwd, ctx.mode, &ctx.blocked_write_roots)?;
    atomic_write(&path, args.content.as_bytes())?;
    Ok(format!(
        "wrote {} bytes to {}",
        args.content.len(),
        path.display()
    ))
}

pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let tmp = parent.join(format!(".{file_name}.cass-tmp-{}", nanoid!(8)));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        bail!(err);
    }
    Ok(())
}
