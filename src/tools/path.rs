use crate::access::AccessMode;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

pub fn resolve_existing(input: &str, cwd: &Path, root: &Path, mode: AccessMode) -> Result<PathBuf> {
    let p = expand_tilde(input);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    let canon = fs::canonicalize(&abs).with_context(|| format!("resolving {}", abs.display()))?;
    if matches!(mode, AccessMode::ReadOnly) {
        let root_canon =
            fs::canonicalize(root).with_context(|| format!("resolving root {}", root.display()))?;
        if !canon.starts_with(&root_canon) {
            bail!("path escapes read-only root: {}", abs.display());
        }
    }
    Ok(canon)
}

pub fn resolve_for_write(input: &str, cwd: &Path, mode: AccessMode) -> Result<PathBuf> {
    if !mode.can_write() {
        bail!("write access requires full-access mode");
    }
    let p = expand_tilde(input);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    Ok(normalize_lexical(&abs))
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}
