use crate::access::AccessMode;
use crate::security::{
    canonicalize_existing, canonicalize_for_create_or_write, is_under_any_root, normalize_lexical,
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

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
    let canon = canonicalize_existing(&abs)?;
    if matches!(mode, AccessMode::ReadOnly | AccessMode::WorkspaceEdit) {
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
    if matches!(mode, AccessMode::ReadOnly) {
        bail!("write access is unavailable in read-only mode");
    }
    let p = expand_tilde(input);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    let normalized = normalize_lexical(&abs);
    ensure_not_under_any_root(&normalized, blocked_write_roots)?;
    if matches!(mode, AccessMode::WorkspaceEdit) {
        let canon = canonicalize_for_create_or_write(&normalized)?;
        let workspace = canonicalize_existing(cwd)?;
        if !canon.starts_with(&workspace) {
            bail!(
                "write path escapes workspace-edit root: {} (workspace root: {})",
                canon.display(),
                workspace.display()
            );
        }
    }
    Ok(normalized)
}

fn ensure_under_any_root(path: &Path, roots: &[PathBuf]) -> Result<()> {
    for root in roots {
        let root_canon = canonicalize_existing(root)?;
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

    let canon = canonicalize_for_create_or_write(path)?;
    let canon_roots: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| canonicalize_for_create_or_write(root).ok())
        .collect();
    if is_under_any_root(&canon, &canon_roots) {
        bail!(
            "writes are blocked under read-only docs directory: {}",
            canon.display()
        );
    }
    Ok(())
}

pub fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}
