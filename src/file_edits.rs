use crate::tools::{self, ToolContext};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_SNAPSHOT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEditJournalEntry {
    pub chat_id: String,
    pub record_index: usize,
    pub tool_call_id: String,
    pub tool_name: String,
    pub path: PathBuf,
    pub existed_before: bool,
    pub existed_after: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_snapshot: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_snapshot: Option<PathBuf>,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub struct PendingFileEditSnapshot {
    pub chat_id: String,
    pub record_index: usize,
    pub tool_call_id: String,
    pub tool_name: String,
    pub path: PathBuf,
    before: SnapshotState,
}

#[derive(Debug, Clone)]
enum SnapshotState {
    Missing,
    File { bytes: Vec<u8>, hash: String },
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    pub actions: Vec<RestoreAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreAction {
    Write {
        path: PathBuf,
        snapshot: PathBuf,
        desired_hash: String,
        expected_current_hash: Option<String>,
        conflict: bool,
    },
    Delete {
        path: PathBuf,
        expected_current_hash: Option<String>,
        conflict: bool,
    },
    Skip {
        path: PathBuf,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

pub fn begin_tool_edit(
    cass_root: &Path,
    chat_id: &str,
    record_index: usize,
    tool_call_id: &str,
    tool_name: &str,
    args: &Value,
    ctx: &ToolContext,
) -> Option<PendingFileEditSnapshot> {
    if !matches!(tool_name, "write" | "edit") {
        return None;
    }
    let path_arg = args.get("path")?.as_str()?;
    let path =
        tools::path::resolve_for_write(path_arg, &ctx.cwd, ctx.mode, &ctx.blocked_write_roots)
            .ok()?;
    let before = snapshot_state(&path).unwrap_or(SnapshotState::Unsupported);
    // Ensure journal directories are creatable before executing, but do not fail
    // the tool if Cassady cannot journal; restore will simply be unavailable.
    let _ = fs::create_dir_all(cass_root.join("file-edits"));
    let _ = fs::create_dir_all(cass_root.join("file-snapshots"));
    Some(PendingFileEditSnapshot {
        chat_id: chat_id.to_string(),
        record_index,
        tool_call_id: tool_call_id.to_string(),
        tool_name: tool_name.to_string(),
        path,
        before,
    })
}

pub fn finish_tool_edit(cass_root: &Path, pending: PendingFileEditSnapshot) -> Result<()> {
    let after = snapshot_state(&pending.path).unwrap_or(SnapshotState::Unsupported);
    if matches!(pending.before, SnapshotState::Unsupported)
        || matches!(after, SnapshotState::Unsupported)
    {
        return Ok(());
    }
    if same_state(&pending.before, &after) {
        return Ok(());
    }

    let (existed_before, before_hash, before_snapshot) = store_snapshot(
        cass_root,
        &pending.chat_id,
        &pending.tool_call_id,
        "before",
        &pending.before,
    )?;
    let (existed_after, after_hash, after_snapshot) = store_snapshot(
        cass_root,
        &pending.chat_id,
        &pending.tool_call_id,
        "after",
        &after,
    )?;

    let entry = FileEditJournalEntry {
        chat_id: pending.chat_id.clone(),
        record_index: pending.record_index,
        tool_call_id: pending.tool_call_id,
        tool_name: pending.tool_name,
        path: pending.path,
        existed_before,
        existed_after,
        before_hash,
        after_hash,
        before_snapshot,
        after_snapshot,
        ts: crate::conversation::now_ts(),
    };
    append_journal(cass_root, &pending.chat_id, &entry)
}

pub fn load_journal(cass_root: &Path, chat_id: &str) -> Result<Vec<FileEditJournalEntry>> {
    let path = journal_path(cass_root, chat_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: FileEditJournalEntry = serde_json::from_str(line)
            .with_context(|| format!("parsing {} line {}", path.display(), idx + 1))?;
        out.push(entry);
    }
    out.sort_by_key(|entry| entry.record_index);
    Ok(out)
}

pub fn plan_restore(
    cass_root: &Path,
    chat_id: &str,
    target_record_index: usize,
) -> Result<RestorePlan> {
    let entries = load_journal(cass_root, chat_id)?;
    let mut by_path: BTreeMap<PathBuf, Vec<FileEditJournalEntry>> = BTreeMap::new();
    for entry in entries {
        by_path.entry(entry.path.clone()).or_default().push(entry);
    }

    let mut actions = Vec::new();
    for (path, mut entries) in by_path {
        entries.sort_by_key(|entry| entry.record_index);
        let latest = entries.last().cloned();
        let desired = entries
            .iter()
            .rev()
            .find(|entry| entry.record_index <= target_record_index)
            .cloned();
        let first_after = entries
            .iter()
            .find(|entry| entry.record_index > target_record_index)
            .cloned();

        let (want_exists, want_hash, want_snapshot) = if let Some(entry) = desired {
            (entry.existed_after, entry.after_hash, entry.after_snapshot)
        } else if let Some(entry) = first_after {
            (
                entry.existed_before,
                entry.before_hash,
                entry.before_snapshot,
            )
        } else {
            continue;
        };

        let expected_current_hash = latest.and_then(|entry| entry.after_hash);
        let current_hash = hash_existing_file(&path)?;
        let conflict = expected_current_hash.is_some()
            && current_hash.is_some()
            && expected_current_hash != current_hash;

        if want_exists {
            match (want_hash, want_snapshot) {
                (Some(desired_hash), Some(snapshot)) => actions.push(RestoreAction::Write {
                    path,
                    snapshot,
                    desired_hash,
                    expected_current_hash,
                    conflict,
                }),
                _ => actions.push(RestoreAction::Skip {
                    path,
                    reason: "missing desired snapshot".into(),
                }),
            }
        } else {
            let conflict = conflict
                || (current_hash.is_some()
                    && expected_current_hash.is_none()
                    && current_hash != expected_current_hash);
            actions.push(RestoreAction::Delete {
                path,
                expected_current_hash,
                conflict,
            });
        }
    }

    Ok(RestorePlan { actions })
}

pub fn apply_restore_plan(plan: &RestorePlan) -> Result<RestoreOutcome> {
    let mut outcome = RestoreOutcome {
        applied: 0,
        skipped: 0,
        conflicts: 0,
    };
    for action in &plan.actions {
        match action {
            RestoreAction::Write {
                path,
                snapshot,
                conflict,
                ..
            } => {
                if *conflict {
                    outcome.conflicts += 1;
                    continue;
                }
                let bytes = fs::read(snapshot)
                    .with_context(|| format!("reading snapshot {}", snapshot.display()))?;
                crate::tools::write::atomic_write(path, &bytes)
                    .with_context(|| format!("restoring {}", path.display()))?;
                outcome.applied += 1;
            }
            RestoreAction::Delete { path, conflict, .. } => {
                if *conflict {
                    outcome.conflicts += 1;
                    continue;
                }
                if path.exists() {
                    fs::remove_file(path)
                        .with_context(|| format!("deleting {}", path.display()))?;
                    outcome.applied += 1;
                } else {
                    outcome.skipped += 1;
                }
            }
            RestoreAction::Skip { .. } => outcome.skipped += 1,
        }
    }
    Ok(outcome)
}

pub fn summarize_plan(plan: &RestorePlan) -> String {
    if plan.actions.is_empty() {
        return "No tracked file edits need restoration for this checkpoint.".into();
    }
    let mut lines = Vec::new();
    for action in &plan.actions {
        match action {
            RestoreAction::Write { path, conflict, .. } => lines.push(format!(
                "{} update {}",
                if *conflict { "CONFLICT" } else { "will" },
                path.display()
            )),
            RestoreAction::Delete { path, conflict, .. } => lines.push(format!(
                "{} delete {}",
                if *conflict { "CONFLICT" } else { "will" },
                path.display()
            )),
            RestoreAction::Skip { path, reason } => {
                lines.push(format!("skip {}: {reason}", path.display()))
            }
        }
    }
    lines.join("\n")
}

fn snapshot_state(path: &Path) -> Result<SnapshotState> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
                return Ok(SnapshotState::Unsupported);
            }
            let bytes = fs::read(path)?;
            let hash = sha256_hex(&bytes);
            Ok(SnapshotState::File { bytes, hash })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SnapshotState::Missing),
        Err(err) => Err(err.into()),
    }
}

fn same_state(a: &SnapshotState, b: &SnapshotState) -> bool {
    match (a, b) {
        (SnapshotState::Missing, SnapshotState::Missing) => true,
        (SnapshotState::File { hash: a, .. }, SnapshotState::File { hash: b, .. }) => a == b,
        _ => false,
    }
}

fn store_snapshot(
    cass_root: &Path,
    chat_id: &str,
    tool_call_id: &str,
    side: &str,
    state: &SnapshotState,
) -> Result<(bool, Option<String>, Option<PathBuf>)> {
    match state {
        SnapshotState::Missing => Ok((false, None, None)),
        SnapshotState::Unsupported => Ok((false, None, None)),
        SnapshotState::File { bytes, hash } => {
            let dir = cass_root
                .join("file-snapshots")
                .join(chat_id)
                .join(tool_call_id);
            fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{side}-{hash}.bin"));
            if !path.exists() {
                fs::write(&path, bytes)?;
            }
            Ok((true, Some(hash.clone()), Some(path)))
        }
    }
}

fn append_journal(cass_root: &Path, chat_id: &str, entry: &FileEditJournalEntry) -> Result<()> {
    let path = journal_path(cass_root, chat_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    file.flush()?;
    Ok(())
}

fn journal_path(cass_root: &Path, chat_id: &str) -> PathBuf {
    cass_root
        .join("file-edits")
        .join(format!("{chat_id}.jsonl"))
}

fn hash_existing_file(path: &Path) -> Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
                return Ok(None);
            }
            Ok(Some(sha256_hex(&fs::read(path)?)))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::AccessMode;
    use tempfile::tempdir;

    fn tool_ctx(cwd: &Path) -> ToolContext {
        ToolContext {
            mode: AccessMode::WorkspaceEdit,
            cwd: cwd.to_path_buf(),
            read_roots: vec![cwd.to_path_buf()],
            blocked_write_roots: Vec::new(),
            model_result_limit: 1000,
            runtime_tx: None,
        }
    }

    #[test]
    fn journal_and_restore_rewinds_write() {
        let root = tempdir().unwrap();
        let work = tempdir().unwrap();
        let path = work.path().join("a.txt");
        fs::write(&path, "old").unwrap();
        let ctx = tool_ctx(work.path());
        let pending = begin_tool_edit(
            root.path(),
            "chat",
            3,
            "call",
            "write",
            &serde_json::json!({"path":"a.txt"}),
            &ctx,
        )
        .unwrap();
        fs::write(&path, "new").unwrap();
        finish_tool_edit(root.path(), pending).unwrap();

        let plan = plan_restore(root.path(), "chat", 2).unwrap();
        assert_eq!(plan.actions.len(), 1);
        let outcome = apply_restore_plan(&plan).unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    }

    #[test]
    fn restore_detects_external_conflict() {
        let root = tempdir().unwrap();
        let work = tempdir().unwrap();
        let path = work.path().join("a.txt");
        fs::write(&path, "old").unwrap();
        let ctx = tool_ctx(work.path());
        let pending = begin_tool_edit(
            root.path(),
            "chat",
            3,
            "call",
            "write",
            &serde_json::json!({"path":"a.txt"}),
            &ctx,
        )
        .unwrap();
        fs::write(&path, "new").unwrap();
        finish_tool_edit(root.path(), pending).unwrap();
        fs::write(&path, "manual").unwrap();
        let plan = plan_restore(root.path(), "chat", 2).unwrap();
        assert!(matches!(
            &plan.actions[0],
            RestoreAction::Write { conflict: true, .. }
        ));
    }
}
