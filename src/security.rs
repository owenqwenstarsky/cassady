use crate::access::AccessMode;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub mode: AccessMode,
    pub cwd: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub blocked_write_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ToolAction {
    List { path: PathBuf },
    Read { path: PathBuf },
    Search { path: PathBuf },
    Write { path: PathBuf },
    Edit { path: PathBuf },
    Shell { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Ask { reason: String },
    Deny { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAvailability {
    Unavailable,
    Available,
    RequiresApproval,
}

pub struct SecurityPolicy;

impl SecurityPolicy {
    pub fn tool_availability(ctx: &SecurityContext, tool: &str) -> ToolAvailability {
        match tool {
            "ls" | "read" | "grep" => ToolAvailability::Available,
            "write" | "edit" => match ctx.mode {
                AccessMode::ReadOnly => ToolAvailability::Unavailable,
                AccessMode::WorkspaceEdit | AccessMode::FullAccess => ToolAvailability::Available,
            },
            "shell" => match ctx.mode {
                AccessMode::ReadOnly => ToolAvailability::Unavailable,
                AccessMode::WorkspaceEdit => ToolAvailability::RequiresApproval,
                AccessMode::FullAccess => ToolAvailability::Available,
            },
            _ => ToolAvailability::Unavailable,
        }
    }

    pub fn check(ctx: &SecurityContext, action: &ToolAction) -> PolicyDecision {
        match action {
            ToolAction::List { path } | ToolAction::Read { path } | ToolAction::Search { path } => {
                check_read(ctx, path)
            }
            ToolAction::Write { path } | ToolAction::Edit { path } => check_write(ctx, path),
            ToolAction::Shell { command } => match ctx.mode {
                AccessMode::ReadOnly => PolicyDecision::Deny {
                    reason: "shell is unavailable in read-only mode".into(),
                },
                AccessMode::WorkspaceEdit => PolicyDecision::Ask {
                    reason: format!(
                        "shell commands require user approval in workspace-edit mode: {command}"
                    ),
                },
                AccessMode::FullAccess => PolicyDecision::Allow,
            },
        }
    }
}

fn check_read(ctx: &SecurityContext, path: &Path) -> PolicyDecision {
    match ctx.mode {
        AccessMode::ReadOnly | AccessMode::WorkspaceEdit => match canonicalize_existing(path) {
            Ok(path) if is_under_any_root(&path, &canonical_roots(&ctx.read_roots)) => {
                PolicyDecision::Allow
            }
            Ok(path) => PolicyDecision::Deny {
                reason: format!(
                    "path escapes read-only roots: {} (allowed roots: {})",
                    path.display(),
                    roots_display(&ctx.read_roots)
                ),
            },
            Err(err) => PolicyDecision::Deny {
                reason: err.to_string(),
            },
        },
        AccessMode::FullAccess => PolicyDecision::Allow,
    }
}

fn check_write(ctx: &SecurityContext, path: &Path) -> PolicyDecision {
    if matches!(ctx.mode, AccessMode::ReadOnly) {
        return PolicyDecision::Deny {
            reason: "write/edit tools are unavailable in read-only mode".into(),
        };
    }

    let canonical = match canonicalize_for_create_or_write(path) {
        Ok(path) => path,
        Err(err) => {
            return PolicyDecision::Deny {
                reason: err.to_string(),
            }
        }
    };

    let blocked = canonical_roots(&ctx.blocked_write_roots);
    if is_under_any_root(&canonical, &blocked)
        || is_under_any_root(&normalize_lexical(path), &ctx.blocked_write_roots)
    {
        return PolicyDecision::Deny {
            reason: format!(
                "writes are blocked under read-only docs directory: {}",
                canonical.display()
            ),
        };
    }

    if matches!(ctx.mode, AccessMode::WorkspaceEdit) {
        let workspace = canonical_roots(std::slice::from_ref(&ctx.cwd));
        if !is_under_any_root(&canonical, &workspace) {
            return PolicyDecision::Deny {
                reason: format!(
                    "write path escapes workspace-edit root: {} (workspace root: {})",
                    canonical.display(),
                    ctx.cwd.display()
                ),
            };
        }
    }

    PolicyDecision::Allow
}

pub fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))
}

pub fn canonicalize_for_create_or_write(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return canonicalize_existing(path);
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

pub fn normalize_lexical(path: &Path) -> PathBuf {
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

pub fn is_under_root(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub fn is_under_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| is_under_root(path, root))
}

fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .collect()
}

fn roots_display(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        return "<none>".into();
    }
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx(root: &Path, docs: &Path, mode: AccessMode) -> SecurityContext {
        SecurityContext {
            mode,
            cwd: root.to_path_buf(),
            read_roots: vec![root.to_path_buf(), docs.to_path_buf()],
            blocked_write_roots: vec![docs.to_path_buf()],
        }
    }

    #[test]
    fn workspace_edit_asks_for_shell_and_allows_workspace_writes() {
        let root = tempdir().unwrap();
        let docs = tempdir().unwrap();
        let ctx = ctx(root.path(), docs.path(), AccessMode::WorkspaceEdit);
        assert_eq!(
            SecurityPolicy::tool_availability(&ctx, "shell"),
            ToolAvailability::RequiresApproval
        );
        assert!(matches!(
            SecurityPolicy::check(
                &ctx,
                &ToolAction::Shell {
                    command: "pwd".into()
                }
            ),
            PolicyDecision::Ask { .. }
        ));
        assert_eq!(
            SecurityPolicy::check(
                &ctx,
                &ToolAction::Write {
                    path: root.path().join("x")
                }
            ),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn workspace_edit_denies_writes_outside_workspace_and_under_docs() {
        let root = tempdir().unwrap();
        let docs = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let ctx = ctx(root.path(), docs.path(), AccessMode::WorkspaceEdit);
        assert!(matches!(
            SecurityPolicy::check(
                &ctx,
                &ToolAction::Write {
                    path: outside.path().join("x")
                }
            ),
            PolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            SecurityPolicy::check(
                &ctx,
                &ToolAction::Write {
                    path: docs.path().join("x")
                }
            ),
            PolicyDecision::Deny { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn workspace_edit_denies_symlink_escape_writes() {
        let root = tempdir().unwrap();
        let docs = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("outside_link")).unwrap();
        let ctx = ctx(root.path(), docs.path(), AccessMode::WorkspaceEdit);
        assert!(matches!(
            SecurityPolicy::check(
                &ctx,
                &ToolAction::Write {
                    path: root.path().join("outside_link/new")
                }
            ),
            PolicyDecision::Deny { .. }
        ));
    }
}
