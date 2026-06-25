use crate::conversation::{self, BranchPoint, Conversation, Record, StoredToolCall};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

const TOOL_CANCELLED_MESSAGE: &str = "Tool execution cancelled by user.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    User,
    Assistant,
    ToolCall,
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: String,
    pub chat_id: String,
    pub record_index: usize,
    pub tool_call_id: Option<String>,
    pub kind: CheckpointKind,
    pub label: String,
    pub detail: String,
    pub ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSummary {
    pub id: String,
    pub created_at: String,
    pub parent_chat_id: Option<String>,
    pub branch_label: Option<String>,
    pub record_count: usize,
    pub current: bool,
}

#[derive(Debug, Clone)]
pub struct BranchFamily {
    pub branches: Vec<BranchSummary>,
    pub checkpoints: Vec<Checkpoint>,
}

pub fn checkpoint_records(records: &[Record], chat_id: &str) -> Vec<Checkpoint> {
    let mut checkpoints = Vec::new();
    for (idx, record) in records.iter().enumerate() {
        match record {
            Record::User { content, ts } => checkpoints.push(Checkpoint {
                id: format!("{chat_id}:{idx}:user"),
                chat_id: chat_id.to_string(),
                record_index: idx,
                tool_call_id: None,
                kind: CheckpointKind::User,
                label: "user".into(),
                detail: preview(content),
                ts: Some(ts.clone()),
            }),
            Record::Assistant {
                content,
                reasoning,
                tool_calls,
                ts,
                ..
            } => {
                let detail = if content.trim().is_empty() {
                    preview(reasoning)
                } else {
                    preview(content)
                };
                checkpoints.push(Checkpoint {
                    id: format!("{chat_id}:{idx}:assistant"),
                    chat_id: chat_id.to_string(),
                    record_index: idx,
                    tool_call_id: None,
                    kind: CheckpointKind::Assistant,
                    label: "assistant".into(),
                    detail,
                    ts: Some(ts.clone()),
                });
                for call in tool_calls {
                    checkpoints.push(Checkpoint {
                        id: format!("{chat_id}:{idx}:tool_call:{}", call.id),
                        chat_id: chat_id.to_string(),
                        record_index: idx,
                        tool_call_id: Some(call.id.clone()),
                        kind: CheckpointKind::ToolCall,
                        label: format!("tool call {}", call.name),
                        detail: tool_call_detail(call),
                        ts: Some(ts.clone()),
                    });
                }
            }
            Record::Tool {
                tool_call_id,
                name,
                ok,
                content,
                ts,
            } => checkpoints.push(Checkpoint {
                id: format!("{chat_id}:{idx}:tool_result:{tool_call_id}"),
                chat_id: chat_id.to_string(),
                record_index: idx,
                tool_call_id: Some(tool_call_id.clone()),
                kind: CheckpointKind::ToolResult,
                label: format!("tool {name} {}", if *ok { "✓" } else { "✗" }),
                detail: preview(content),
                ts: Some(ts.clone()),
            }),
            _ => {}
        }
    }
    checkpoints
}

pub fn load_family(conversations_dir: &Path, current: &Conversation) -> Result<BranchFamily> {
    let metas = load_all_metas(conversations_dir)?;
    let root = root_for(&current.id, &metas);
    let mut ids = BTreeSet::new();
    for id in metas.keys() {
        if root_for(id, &metas) == root {
            ids.insert(id.clone());
        }
    }
    ids.insert(current.id.clone());

    let mut branches = Vec::new();
    let mut checkpoints = Vec::new();
    for id in ids {
        let Ok((conversation, _)) = Conversation::load(conversations_dir, &id) else {
            continue;
        };
        let meta = conversation.meta();
        branches.push(BranchSummary {
            id: conversation.id.clone(),
            created_at: meta
                .as_ref()
                .map(|m| m.created_at.clone())
                .unwrap_or_default(),
            parent_chat_id: meta.as_ref().and_then(|m| m.parent_chat_id.clone()),
            branch_label: meta
                .as_ref()
                .and_then(|m| m.branch_from.as_ref().map(|p| p.checkpoint_label.clone())),
            record_count: conversation.records.len(),
            current: conversation.id == current.id,
        });
        checkpoints.extend(checkpoint_records(&conversation.records, &conversation.id));
    }
    branches.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    checkpoints.sort_by(|a, b| {
        a.chat_id
            .cmp(&b.chat_id)
            .then(a.record_index.cmp(&b.record_index))
            .then(a.id.cmp(&b.id))
    });
    Ok(BranchFamily {
        branches,
        checkpoints,
    })
}

pub fn create_branch(
    conversations_dir: &Path,
    source: &Conversation,
    checkpoint: &Checkpoint,
) -> Result<Conversation> {
    if source.id != checkpoint.chat_id {
        bail!(
            "checkpoint {} belongs to {}, not {}",
            checkpoint.id,
            checkpoint.chat_id,
            source.id
        );
    }
    if checkpoint.record_index >= source.records.len() {
        bail!("checkpoint record index is out of range");
    }

    fs::create_dir_all(conversations_dir)?;
    let id = conversation::new_chat_id();
    let path = conversations_dir.join(format!("{id}.jsonl"));
    let mut records = Vec::new();
    let meta = source
        .meta()
        .context("source conversation is missing metadata")?;
    records.push(Record::Meta {
        chat_id: id.clone(),
        created_at: conversation::now_ts(),
        model: meta.model,
        cwd: meta.cwd,
        parent_chat_id: Some(source.id.clone()),
        branch_from: Some(BranchPoint {
            chat_id: source.id.clone(),
            record_index: checkpoint.record_index,
            tool_call_id: checkpoint.tool_call_id.clone(),
            checkpoint_label: checkpoint_title(checkpoint),
        }),
    });

    let prefix = valid_prefix(source, checkpoint)?;
    records.extend(prefix);
    repair_pending_tool_calls(&mut records);

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("creating branch conversation {}", path.display()))?;
    for record in &records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    file.flush()?;

    Ok(Conversation { id, path, records })
}

fn valid_prefix(source: &Conversation, checkpoint: &Checkpoint) -> Result<Vec<Record>> {
    let mut end = checkpoint.record_index + 1;
    if matches!(checkpoint.kind, CheckpointKind::ToolCall) {
        end = checkpoint.record_index + 1;
    }
    let mut prefix = source.records[..end].to_vec();

    // Drop the source meta; the branch writes its own meta record.
    if matches!(prefix.first(), Some(Record::Meta { .. })) {
        prefix.remove(0);
    }

    // For a tool-call checkpoint, keep the assistant turn but do not copy any
    // later tool result. The repair step below writes cancelled tool results so
    // the next provider request remains valid.
    Ok(prefix)
}

pub fn repair_pending_tool_calls(records: &mut Vec<Record>) {
    let mut pending: Vec<(String, String)> = Vec::new();
    for record in records.iter() {
        match record {
            Record::Assistant { tool_calls, .. } => {
                pending = tool_calls
                    .iter()
                    .map(|call| (call.id.clone(), call.name.clone()))
                    .collect();
            }
            Record::Tool { tool_call_id, .. } => pending.retain(|(id, _)| id != tool_call_id),
            Record::User { .. } => pending.clear(),
            _ => {}
        }
    }
    let seen: HashSet<String> = records
        .iter()
        .filter_map(|record| match record {
            Record::Tool { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();
    for (id, name) in pending {
        if seen.contains(&id) {
            continue;
        }
        records.push(Record::Tool {
            tool_call_id: id,
            name,
            ok: false,
            content: TOOL_CANCELLED_MESSAGE.to_string(),
            ts: conversation::now_ts(),
        });
    }
}

fn load_all_metas(conversations_dir: &Path) -> Result<BTreeMap<String, (Option<String>, String)>> {
    let mut out = BTreeMap::new();
    if !conversations_dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(conversations_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(Some((parent, cwd))) = read_meta_parent_cwd(&path) {
            out.insert(id.to_string(), (parent, cwd));
        }
    }
    Ok(out)
}

fn read_meta_parent_cwd(path: &Path) -> Result<Option<(Option<String>, String)>> {
    let file = File::open(path)?;
    for line in BufReader::new(file).lines().take(10) {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = serde_json::from_str(&line)?;
        if let Record::Meta {
            parent_chat_id,
            cwd,
            ..
        } = record
        {
            return Ok(Some((parent_chat_id, cwd)));
        }
    }
    Ok(None)
}

fn root_for(id: &str, metas: &BTreeMap<String, (Option<String>, String)>) -> String {
    let mut current = id.to_string();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let Some((Some(parent), _)) = metas.get(&current) else {
            break;
        };
        current = parent.clone();
    }
    current
}

pub fn checkpoint_title(checkpoint: &Checkpoint) -> String {
    if checkpoint.detail.is_empty() {
        checkpoint.label.clone()
    } else {
        format!("{}: {}", checkpoint.label, checkpoint.detail)
    }
}

fn tool_call_detail(call: &StoredToolCall) -> String {
    let mut detail = String::new();
    if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
        detail = format!("file: {path}");
    } else if let Some(command) = call.arguments.get("command").and_then(|v| v.as_str()) {
        detail = command.to_string();
    }
    if detail.is_empty() {
        preview(&call.arguments.to_string())
    } else {
        preview(&detail)
    }
}

fn preview(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(96)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn base_records(id: &str) -> Vec<Record> {
        vec![
            Record::Meta {
                chat_id: id.into(),
                created_at: "now".into(),
                model: "m".into(),
                cwd: "/tmp".into(),
                parent_chat_id: None,
                branch_from: None,
            },
            Record::System {
                content: "s".into(),
            },
            Record::User {
                content: "u".into(),
                ts: "t".into(),
            },
            Record::Assistant {
                content: "a".into(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: vec![StoredToolCall {
                    id: "call1".into(),
                    name: "read".into(),
                    arguments: json!({"path":"x"}),
                }],
                ts: "t".into(),
            },
        ]
    }

    #[test]
    fn checkpoints_include_tool_calls() {
        let checkpoints = checkpoint_records(&base_records("c"), "c");
        assert!(checkpoints.iter().any(|c| c.kind == CheckpointKind::User));
        assert!(checkpoints
            .iter()
            .any(|c| c.kind == CheckpointKind::Assistant));
        assert!(checkpoints
            .iter()
            .any(|c| c.kind == CheckpointKind::ToolCall));
    }

    #[test]
    fn create_branch_does_not_modify_source_and_repairs_pending_tools() {
        let dir = tempdir().unwrap();
        let source = Conversation {
            id: "source".into(),
            path: dir.path().join("source.jsonl"),
            records: base_records("source"),
        };
        let checkpoint = checkpoint_records(&source.records, &source.id)
            .into_iter()
            .find(|c| c.kind == CheckpointKind::Assistant)
            .unwrap();
        let branch = create_branch(dir.path(), &source, &checkpoint).unwrap();
        assert_ne!(branch.id, source.id);
        assert!(
            source
                .records
                .iter()
                .filter(|r| matches!(r, Record::Tool { .. }))
                .count()
                == 0
        );
        assert!(branch
            .records
            .iter()
            .any(|r| matches!(r, Record::Tool { ok: false, .. })));
    }
}
