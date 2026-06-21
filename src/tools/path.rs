use crate::access::AccessMode;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
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

pub fn resolve_existing(
    input: &str,
    cwd: &Path,
    read_roots: &[PathBuf],
    mode: AccessMode,
) -> Result<PathBuf> {
    let p = expand_tilde(input);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    let canon = fs::canonicalize(&abs).with_context(|| format!("resolving {}", abs.display()))?;
    if matches!(mode, AccessMode::ReadOnly) {
        ensure_under_any_root(&canon, read_roots)?;
    }
    Ok(canon)
}

pub fn resolve_for_write(
    input: &str,
    cwd: &Path,
    mode: AccessMode,
    blocked_write_roots: &[PathBuf],
) -> Result<PathBuf> {
    if !mode.can_write() {
        bail!("write access requires full-access mode");
    }
    let p = expand_tilde(input);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    let normalized = normalize_lexical(&abs);
    ensure_not_under_any_root(&normalized, blocked_write_roots)?;
    Ok(normalized)
}

fn ensure_under_any_root(path: &Path, roots: &[PathBuf]) -> Result<()> {
    for root in roots {
        let root_canon = fs::canonicalize(root)
            .with_context(|| format!("resolving read root {}", root.display()))?;
        if path.starts_with(&root_canon) {
            return Ok(());
        }
    }

    let allowed = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "path escapes read-only roots: {} (allowed roots: {})",
        path.display(),
        if allowed.is_empty() {
            "<none>"
        } else {
            &allowed
        }
    )
}

fn ensure_not_under_any_root(path: &Path, roots: &[PathBuf]) -> Result<()> {
    if roots.is_empty() {
        return Ok(());
    }

    let lexical = normalize_lexical(path);
    for root in roots {
        let root_lexical = normalize_lexical(root);
        if lexical.starts_with(&root_lexical) {
            bail!(
                "writes are blocked under read-only docs directory: {}",
                root_lexical.display()
            );
        }
    }

    let canon = canonicalize_for_policy(path)?;
    for root in roots {
        let root_canon = canonicalize_for_policy(root)
            .with_context(|| format!("resolving blocked write root {}", root.display()))?;
        if canon.starts_with(&root_canon) {
            bail!(
                "writes are blocked under read-only docs directory: {}",
                root_canon.display()
            );
        }
    }
    Ok(())
}

fn canonicalize_for_policy(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()));
    }

    let mut missing = Vec::<OsString>::new();
    let mut ancestor = path;
    loop {
        if ancestor.exists() {
            let mut out = fs::canonicalize(ancestor)
                .with_context(|| format!("resolving {}", ancestor.display()))?;
            for component in missing.iter().rev() {
                out.push(component);
            }
            return Ok(normalize_lexical(&out));
        }

        let Some(name) = ancestor.file_name() else {
            bail!("no existing ancestor for {}", path.display());
        };
        missing.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no existing ancestor for {}", path.display()))?;
    }
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
