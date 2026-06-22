use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Record {
    Meta {
        chat_id: String,
        created_at: String,
        model: String,
        cwd: String,
    },
    System {
        content: String,
    },
    User {
        content: String,
        ts: String,
    },
    Assistant {
        content: String,
        #[serde(default)]
        reasoning: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_field: Option<String>,
        tool_calls: Vec<StoredToolCall>,
        ts: String,
    },
    Tool {
        tool_call_id: String,
        name: String,
        ok: bool,
        content: String,
        ts: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub path: PathBuf,
    pub records: Vec<Record>,
}

#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: String,
    pub created_at: String,
    pub model: String,
    pub cwd: String,
    pub first_user_preview: String,
}

pub fn new_chat_id() -> String {
    let ts: DateTime<Local> = Local::now();
    format!("{}-{}", ts.format("%Y-%m-%d-%H%M%S"), nanoid!(4))
}

pub fn now_ts() -> String {
    Utc::now().to_rfc3339()
}

impl Conversation {
    pub fn create(
        conversations_dir: &Path,
        model: &str,
        cwd: &Path,
        base_system: String,
    ) -> Result<Self> {
        fs::create_dir_all(conversations_dir)?;
        let id = new_chat_id();
        let path = conversations_dir.join(format!("{id}.jsonl"));
        let mut convo = Conversation {
            id: id.clone(),
            path,
            records: Vec::new(),
        };
        convo.append(Record::Meta {
            chat_id: id,
            created_at: now_ts(),
            model: model.to_string(),
            cwd: cwd.display().to_string(),
        })?;
        convo.append(Record::System {
            content: base_system,
        })?;
        Ok(convo)
    }

    pub fn load(conversations_dir: &Path, id: &str) -> Result<(Self, Option<String>)> {
        let path = conversations_dir.join(format!("{id}.jsonl"));
        let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();
        let mut warning = None;
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Record>(&line) {
                Ok(record) => records.push(record),
                Err(err) => {
                    warning = Some(format!(
                        "Stopped loading at corrupted JSONL line {}: {}",
                        idx + 1,
                        err
                    ));
                    break;
                }
            }
        }
        Ok((
            Conversation {
                id: id.to_string(),
                path,
                records,
            },
            warning,
        ))
    }

    pub fn append(&mut self, record: Record) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("opening {} for append", self.path.display()))?;
        let line = serde_json::to_string(&record)?;
        writeln!(file, "{line}")?;
        file.flush()?;
        self.records.push(record);
        Ok(())
    }

    pub fn base_system_prompt(&self) -> String {
        self.records
            .iter()
            .find_map(|r| match r {
                Record::System { content } => Some(content.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn meta_cwd(&self) -> Option<String> {
        self.records.iter().find_map(|r| match r {
            Record::Meta { cwd, .. } => Some(cwd.clone()),
            _ => None,
        })
    }
}

pub fn list_chats(conversations_dir: &Path, cwd: &Path) -> Result<Vec<ChatSummary>> {
    let mut out = Vec::new();
    if !conversations_dir.exists() {
        return Ok(out);
    }
    let cwd_str = cwd.display().to_string();
    for entry in fs::read_dir(conversations_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(summary) = summarize_chat(stem, &path) {
            if summary.cwd == cwd_str {
                out.push(summary);
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

fn summarize_chat(id: &str, path: &Path) -> Result<ChatSummary> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut created_at = String::new();
    let mut model = String::new();
    let mut cwd = String::new();
    let mut first_user_preview = String::new();
    for line in reader.lines().take(100) {
        let line = line?;
        let record: Record = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => break,
        };
        match record {
            Record::Meta {
                created_at: c,
                model: m,
                cwd: w,
                ..
            } => {
                created_at = c;
                model = m;
                cwd = w;
            }
            Record::User { content, .. } if first_user_preview.is_empty() => {
                first_user_preview = content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();
            }
            _ => {}
        }
    }
    Ok(ChatSummary {
        id: id.to_string(),
        created_at,
        model,
        cwd,
        first_user_preview,
    })
}
