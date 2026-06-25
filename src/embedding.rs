//! Experimental Rust embedding API for running Cassady without the TUI.
//!
//! This module provides the first Rust-native surface for embedding Cassady in
//! another application. It reuses Cassady's existing runtime behavior while
//! giving the host application control over event presentation, turn lifecycle,
//! and approval decisions.

use crate::access::AccessMode;
use crate::agent::{self, AgentCommand, AgentEvent, AgentSettings};
use crate::config::{Config, ConfigOverrides, ReasoningEffort};
use crate::conversation::{self, Conversation, Record};
use crate::prompt;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const TURN_CANCELLED_MESSAGE: &str = "Turn cancelled by host.";
const TOOL_CANCELLED_MESSAGE: &str = "Tool execution cancelled by host.";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(#[source] anyhow::Error),
    #[error("conversation error: {0}")]
    Conversation(#[source] anyhow::Error),
    #[error("agent error: {0}")]
    Agent(#[source] anyhow::Error),
    #[error("agent task failed: {0}")]
    Join(#[source] tokio::task::JoinError),
    #[error("turn is already closed")]
    TurnClosed,
    #[error("approval request `{0}` is not pending")]
    ApprovalNotPending(String),
    #[error("turn session state is unavailable")]
    MissingSession,
}

impl Error {
    fn config(err: anyhow::Error) -> Self {
        Self::Config(err)
    }

    fn conversation(err: anyhow::Error) -> Self {
        Self::Conversation(err)
    }

    fn agent(err: anyhow::Error) -> Self {
        Self::Agent(err)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionBuilder {
    config_root: Option<PathBuf>,
    cwd: Option<PathBuf>,
    access_mode: Option<AccessMode>,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    reasoning_effort: Option<ReasoningEffort>,
}

impl SessionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn config_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.config_root = Some(root.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn access_mode(mut self, mode: AccessMode) -> Self {
        self.access_mode = Some(mode);
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn api_key_env(mut self, api_key_env: impl Into<String>) -> Self {
        self.api_key_env = Some(api_key_env.into());
        self
    }

    pub fn reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }

    pub async fn build(self) -> Result<Session> {
        self.new_session().await
    }

    pub async fn new_session(self) -> Result<Session> {
        let PreparedSession {
            config,
            cwd,
            mode,
            reasoning_effort,
        } = self.prepare().await?;
        let conversation = create_new_conversation(&config, &cwd)?;
        Ok(Session {
            config,
            cwd,
            mode,
            reasoning_effort,
            conversation,
            resume_warning: None,
        })
    }

    pub async fn resume(self, chat_id: impl AsRef<str>) -> Result<Session> {
        let PreparedSession {
            config,
            cwd,
            mode,
            reasoning_effort,
        } = self.prepare().await?;
        let (conversation, warning) =
            Conversation::load(&config.conversations_dir(), chat_id.as_ref())
                .map_err(Error::conversation)?;
        Ok(Session {
            config,
            cwd,
            mode,
            reasoning_effort,
            conversation,
            resume_warning: warning,
        })
    }

    async fn prepare(self) -> Result<PreparedSession> {
        let root = self.config_root.unwrap_or_else(crate::config::cass_root);
        let overrides = ConfigOverrides {
            model: self.model,
            base_url: self.base_url,
            api_key_env: self.api_key_env,
            access_mode: self.access_mode,
        };
        let config = Config::load_with_overrides(root, overrides).map_err(Error::config)?;
        config.resolved_api_key().map_err(Error::config)?;
        let cwd = resolve_cwd(self.cwd).map_err(Error::config)?;
        let mode = config.default_access_mode;
        let reasoning_effort = self
            .reasoning_effort
            .unwrap_or(config.reasoning_effort)
            .clamp_for_model(config.model_metadata.as_ref());
        Ok(PreparedSession {
            config,
            cwd,
            mode,
            reasoning_effort,
        })
    }
}

struct PreparedSession {
    config: Config,
    cwd: PathBuf,
    mode: AccessMode,
    reasoning_effort: ReasoningEffort,
}

#[derive(Debug)]
pub struct Session {
    config: Config,
    cwd: PathBuf,
    mode: AccessMode,
    reasoning_effort: ReasoningEffort,
    conversation: Conversation,
    resume_warning: Option<String>,
}

impl Session {
    pub fn id(&self) -> &str {
        &self.conversation.id
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub fn access_mode(&self) -> AccessMode {
        self.mode
    }

    pub fn reasoning_effort(&self) -> ReasoningEffort {
        self.reasoning_effort
    }

    pub fn conversation_path(&self) -> &Path {
        &self.conversation.path
    }

    pub fn records(&self) -> &[Record] {
        &self.conversation.records
    }

    pub fn resume_warning(&self) -> Option<&str> {
        self.resume_warning.as_deref()
    }

    pub fn info(&self) -> ConversationInfo {
        ConversationInfo {
            id: self.conversation.id.clone(),
            cwd: self.cwd.clone(),
            model: self.config.model.clone(),
            access_mode: self.mode,
            reasoning_effort: self.reasoning_effort,
            path: self.conversation.path.clone(),
            record_count: self.conversation.records.len(),
        }
    }

    pub async fn start_turn(self, user_message: impl Into<String>) -> Result<Turn> {
        let message = user_message.into();
        let turn_start_len = self.conversation.records.len();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (command_tx, command_rx) = mpsc::unbounded_channel::<AgentCommand>();
        let settings = AgentSettings {
            config: self.config.clone(),
            cwd: self.cwd.clone(),
            mode: self.mode,
            reasoning_effort: self.reasoning_effort,
        };
        let conversation = self.conversation.clone();
        let task_message = message.clone();
        let handle = tokio::spawn(agent::run_turn_with_commands(
            conversation,
            task_message,
            settings,
            event_tx,
            command_rx,
        ));
        Ok(Turn {
            session: Some(self),
            handle: Some(handle),
            event_rx,
            command_tx: Some(command_tx),
            pending_approvals: BTreeSet::new(),
            turn_start_len,
            user_message: message,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ConversationInfo {
    pub id: String,
    pub cwd: PathBuf,
    pub model: String,
    pub access_mode: AccessMode,
    pub reasoning_effort: ReasoningEffort,
    pub path: PathBuf,
    pub record_count: usize,
}

#[derive(Debug)]
pub struct Turn {
    session: Option<Session>,
    handle: Option<JoinHandle<anyhow::Result<Conversation>>>,
    event_rx: mpsc::UnboundedReceiver<AgentEvent>,
    command_tx: Option<mpsc::UnboundedSender<AgentCommand>>,
    pending_approvals: BTreeSet<String>,
    turn_start_len: usize,
    user_message: String,
}

impl Turn {
    pub async fn next_event(&mut self) -> Result<Option<Event>> {
        match self.event_rx.recv().await {
            Some(event) => {
                let event = Event::from_agent(event);
                match &event {
                    Event::ApprovalRequested(request) => {
                        self.pending_approvals.insert(request.request_id.clone());
                    }
                    Event::ApprovalResolved { request_id, .. } => {
                        self.pending_approvals.remove(request_id);
                    }
                    _ => {}
                }
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    pub fn approve(&mut self, request_id: impl AsRef<str>) -> Result<()> {
        self.resolve_approval(request_id.as_ref(), true)
    }

    pub fn deny(&mut self, request_id: impl AsRef<str>) -> Result<()> {
        self.resolve_approval(request_id.as_ref(), false)
    }

    pub async fn finish(mut self) -> Result<Session> {
        let handle = self.handle.take().ok_or(Error::TurnClosed)?;
        let conversation = match handle.await.map_err(Error::Join)? {
            Ok(conversation) => conversation,
            Err(err) => return Err(Error::agent(err)),
        };
        let mut session = self.session.take().ok_or(Error::MissingSession)?;
        session.conversation = conversation;
        self.command_tx = None;
        Ok(session)
    }

    pub async fn cancel(mut self) -> Result<Session> {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(Ok(conversation)) => {
                    let mut session = self.session.take().ok_or(Error::MissingSession)?;
                    session.conversation = conversation;
                    self.command_tx = None;
                    return Ok(session);
                }
                Ok(Err(err)) => return Err(Error::agent(err)),
                Err(err) if err.is_cancelled() => {}
                Err(err) => return Err(Error::Join(err)),
            }
        }
        let mut session = self.session.take().ok_or(Error::MissingSession)?;
        session.conversation = finalize_cancelled_turn(
            &session.config,
            &session.conversation.id,
            self.turn_start_len,
            &self.user_message,
        )?;
        self.command_tx = None;
        Ok(session)
    }

    fn resolve_approval(&mut self, request_id: &str, approved: bool) -> Result<()> {
        if !self.pending_approvals.remove(request_id) {
            return Err(Error::ApprovalNotPending(request_id.to_string()));
        }
        let tx = self.command_tx.as_ref().ok_or(Error::TurnClosed)?;
        tx.send(AgentCommand::ApprovalDecision {
            request_id: request_id.to_string(),
            approved,
        })
        .map_err(|_| Error::TurnClosed)
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    AssistantChunk(String),
    ReasoningChunk(String),
    ToolCallStarted {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolOutputChunk {
        id: String,
        name: String,
        stream: String,
        content: String,
    },
    ToolResult {
        id: String,
        name: String,
        ok: bool,
        content: String,
    },
    ApprovalRequested(ApprovalRequest),
    ApprovalResolved {
        request_id: String,
        approved: bool,
    },
    Status(String),
    Finished,
}

impl Event {
    fn from_agent(event: AgentEvent) -> Self {
        match event {
            AgentEvent::AssistantChunk(text) => Self::AssistantChunk(text),
            AgentEvent::ReasoningChunk(text) => Self::ReasoningChunk(text),
            AgentEvent::ToolCallStarted {
                id,
                name,
                arguments,
            } => Self::ToolCallStarted {
                id,
                name,
                arguments,
            },
            AgentEvent::ToolOutputChunk {
                id,
                name,
                stream,
                content,
            } => Self::ToolOutputChunk {
                id,
                name,
                stream,
                content,
            },
            AgentEvent::ToolResult {
                id,
                name,
                ok,
                content,
            } => Self::ToolResult {
                id,
                name,
                ok,
                content,
            },
            AgentEvent::ApprovalRequested {
                request_id,
                tool_call_id,
                name,
                arguments,
                reason,
            } => Self::ApprovalRequested(ApprovalRequest {
                request_id,
                tool_call_id,
                name,
                arguments,
                reason,
            }),
            AgentEvent::ApprovalResolved {
                request_id,
                approved,
            } => Self::ApprovalResolved {
                request_id,
                approved,
            },
            AgentEvent::Status(status) => Self::Status(status),
            AgentEvent::TurnFinished => Self::Finished,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub request_id: String,
    pub tool_call_id: String,
    pub name: String,
    pub arguments: Value,
    pub reason: String,
}

fn resolve_cwd(cwd: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let cwd = cwd.unwrap_or(std::env::current_dir()?);
    cwd.canonicalize()
        .map_err(anyhow::Error::from)
        .map_err(|err| anyhow::anyhow!("resolving cwd {}: {err}", cwd.display()))
}

fn create_new_conversation(config: &Config, cwd: &Path) -> Result<Conversation> {
    let global = fs::read_to_string(config.global_path()).ok();
    let base = prompt::build_base_system_prompt(global.as_deref());
    Conversation::create(&config.conversations_dir(), &config.model, cwd, base)
        .map_err(Error::conversation)
}

fn finalize_cancelled_turn(
    config: &Config,
    chat_id: &str,
    turn_start_len: usize,
    turn_message: &str,
) -> Result<Conversation> {
    let (mut conversation, _) =
        Conversation::load(&config.conversations_dir(), chat_id).map_err(Error::conversation)?;

    if conversation.records.len() <= turn_start_len {
        conversation
            .append(Record::User {
                content: turn_message.to_string(),
                ts: conversation::now_ts(),
            })
            .map_err(Error::conversation)?;
    }

    for (id, name) in pending_tool_calls(&conversation.records) {
        conversation
            .append(Record::Tool {
                tool_call_id: id,
                name,
                ok: false,
                content: TOOL_CANCELLED_MESSAGE.to_string(),
                ts: conversation::now_ts(),
            })
            .map_err(Error::conversation)?;
    }

    if !matches!(
        conversation.records.last(),
        Some(Record::Assistant { content, tool_calls, .. })
            if content == TURN_CANCELLED_MESSAGE && tool_calls.is_empty()
    ) {
        conversation
            .append(Record::Assistant {
                content: TURN_CANCELLED_MESSAGE.to_string(),
                reasoning: String::new(),
                reasoning_field: None,
                tool_calls: Vec::new(),
                ts: conversation::now_ts(),
            })
            .map_err(Error::conversation)?;
    }

    Ok(conversation)
}

fn pending_tool_calls(records: &[Record]) -> Vec<(String, String)> {
    let mut pending = Vec::new();
    for record in records {
        match record {
            Record::Assistant { tool_calls, .. } => {
                pending = tool_calls
                    .iter()
                    .map(|call| (call.id.clone(), call.name.clone()))
                    .collect();
            }
            Record::Tool { tool_call_id, .. } => {
                pending.retain(|(id, _)| id != tool_call_id);
            }
            Record::User { .. } => pending.clear(),
            _ => {}
        }
    }
    pending
}
