//! Shared slash-command system for Cassady hosts.
//!
//! This module is the single source of truth for the in-chat slash commands
//! (`/branch`, `/fast`, `/login`, `/logout`, `/model`, `/new`, `/resume`,
//! `/status`). It owns the command catalog, the parser, the autocomplete
//! menu builders, the pure command actions, and a unified `execute` entry
//! point. Both the CLI TUI (`crate::app`) and the Tauri desktop
//! (`cassady-desktop`) drive their slash-command UX through this module so
//! behavior stays in sync.
//!
//! Design rule: core does data, host does picker. Pure commands (`/fast`,
//! `/model`, `/new`, `/resume`, `/status`) execute fully here and return a
//! displayable `CommandOutcome`. Interactive commands (`/login`, `/logout`,
//! `/branch`) return an `Open*Picker` outcome carrying candidate data; the
//! host renders its own picker and calls back into the apply facades below.

use crate::access::AccessMode;
use crate::branch::{self, BranchFamily, Checkpoint};
use crate::config::{
    self, Config, ConfigOverrides, FastModeState, ModelDefinition, ReasoningEffort,
};
use crate::conversation::{self, Conversation};
use crate::file_edits;
use crate::prompt;
use crate::setup::{
    self, LogoutResult, ProviderCatalogEntry, ProviderLogoutCandidate, SetupSelection,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A single autocomplete suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoFillItem {
    pub label: String,
    pub insert: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AutoFillItem {
    pub fn new(label: impl Into<String>, insert: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            insert: insert.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A replacement menu produced by the autocomplete builders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoFillMenu {
    pub title: String,
    pub replacement_start: usize,
    pub replacement_end: usize,
    pub items: Vec<AutoFillItem>,
    pub selected: usize,
}

impl AutoFillMenu {
    pub fn new(
        title: impl Into<String>,
        replacement_start: usize,
        replacement_end: usize,
        items: Vec<AutoFillItem>,
    ) -> Self {
        Self {
            title: title.into(),
            replacement_start,
            replacement_end,
            items,
            selected: 0,
        }
    }

    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(self.items.len().saturating_sub(1));
        self
    }

    pub fn selected_index(&self) -> Option<usize> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.selected.min(self.items.len() - 1))
        }
    }

    pub fn previous_index(&self) -> usize {
        self.selected_index().unwrap_or(0).saturating_sub(1)
    }

    pub fn next_index(&self) -> usize {
        self.selected_index()
            .map(|idx| (idx + 1).min(self.items.len().saturating_sub(1)))
            .unwrap_or(0)
    }

    /// Splice the selected item's `insert` into `input` between the
    /// replacement bounds, returning the new input string.
    pub fn apply(&self, input: &str) -> Option<String> {
        let item = self.items.get(self.selected_index()?)?;
        if self.replacement_start > self.replacement_end || self.replacement_end > input.len() {
            return None;
        }
        if !input.is_char_boundary(self.replacement_start)
            || !input.is_char_boundary(self.replacement_end)
        {
            return None;
        }

        let mut out = String::with_capacity(
            input.len() - (self.replacement_end - self.replacement_start) + item.insert.len(),
        );
        out.push_str(&input[..self.replacement_start]);
        out.push_str(&item.insert);
        out.push_str(&input[self.replacement_end..]);
        Some(out)
    }
}

/// The parsed in-chat command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    Branch,
    Fast(FastModeCommand),
    Login,
    Logout,
    Model(String),
    New,
    Resume(String),
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastModeCommand {
    Toggle,
    On,
    Off,
    Status,
}

/// Catalog metadata for a command, used by autocomplete and help surfaces.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
    pub takes_value: bool,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "branch",
        usage: "/branch",
        description: "open branch/restore menu",
        takes_value: false,
    },
    CommandSpec {
        name: "fast",
        usage: "/fast [on|off|status]",
        description: "toggle faster Codex inference when supported",
        takes_value: false,
    },
    CommandSpec {
        name: "login",
        usage: "/login",
        description: "configure provider login settings",
        takes_value: false,
    },
    CommandSpec {
        name: "logout",
        usage: "/logout",
        description: "remove saved providers and models",
        takes_value: false,
    },
    CommandSpec {
        name: "model",
        usage: "/model <model>",
        description: "switch the model for future turns",
        takes_value: true,
    },
    CommandSpec {
        name: "new",
        usage: "/new",
        description: "create a new chat",
        takes_value: false,
    },
    CommandSpec {
        name: "resume",
        usage: "/resume <chat>",
        description: "resume a chat from this directory",
        takes_value: true,
    },
    CommandSpec {
        name: "status",
        usage: "/status",
        description: "show chat status",
        takes_value: false,
    },
];

/// Parse a typed slash command. Returns a usage error string on bad input.
pub fn parse(input: &str) -> std::result::Result<LocalCommand, String> {
    let trimmed = input.trim();
    let mut parts = trimmed.split_whitespace();
    let Some(command) = parts.next() else {
        return Err("empty command".into());
    };

    match command {
        "/branch" | "/restore" => {
            if parts.next().is_some() {
                return Err("usage: /branch".into());
            }
            Ok(LocalCommand::Branch)
        }
        "/fast" => {
            let command = match parts.next() {
                None => FastModeCommand::Toggle,
                Some("on") => FastModeCommand::On,
                Some("off") => FastModeCommand::Off,
                Some("status") => FastModeCommand::Status,
                Some(_) => return Err("usage: /fast [on|off|status]".into()),
            };
            if parts.next().is_some() {
                return Err("usage: /fast [on|off|status]".into());
            }
            Ok(LocalCommand::Fast(command))
        }
        "/login" => {
            if parts.next().is_some() {
                return Err("usage: /login".into());
            }
            Ok(LocalCommand::Login)
        }
        "/logout" => {
            if parts.next().is_some() {
                return Err("usage: /logout".into());
            }
            Ok(LocalCommand::Logout)
        }
        "/model" => {
            let Some(model) = parts.next() else {
                return Err("usage: /model <model>".into());
            };
            if parts.next().is_some() {
                return Err("usage: /model <model>".into());
            }
            Ok(LocalCommand::Model(model.to_string()))
        }
        "/resume" => {
            let Some(chat) = parts.next() else {
                return Err("usage: /resume <chat>".into());
            };
            if parts.next().is_some() {
                return Err("usage: /resume <chat>".into());
            }
            Ok(LocalCommand::Resume(chat.to_string()))
        }
        "/new" => {
            if parts.next().is_some() {
                return Err("usage: /new".into());
            }
            Ok(LocalCommand::New)
        }
        "/status" => {
            if parts.next().is_some() {
                return Err("usage: /status".into());
            }
            Ok(LocalCommand::Status)
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// Build the autocomplete menu for the current input, if any.
pub fn build_autofill(
    input: &str,
    selected: usize,
    config: &Config,
    cwd: &Path,
) -> Result<Option<AutoFillMenu>> {
    if input.contains('\n') || !input.starts_with('/') {
        return Ok(None);
    }

    if let Some(menu) = command_autofill(input, selected) {
        return Ok(Some(menu));
    }

    if let Some(menu) = model_autofill(input, selected, config)? {
        return Ok(Some(menu));
    }

    resume_chat_autofill(input, selected, config, cwd)
}

/// Command-name autocomplete, shown when the input is a bare `/word`.
pub fn command_autofill(input: &str, selected: usize) -> Option<AutoFillMenu> {
    if !input.starts_with('/') || input[1..].chars().any(char::is_whitespace) {
        return None;
    }
    if COMMANDS
        .iter()
        .any(|spec| !spec.takes_value && input == format!("/{}", spec.name))
    {
        return None;
    }

    let query = input[1..].to_ascii_lowercase();
    let mut items = Vec::new();
    for spec in COMMANDS {
        if spec.name.starts_with(&query) || spec.usage[1..].starts_with(&query) {
            let insert = if spec.takes_value {
                format!("/{} ", spec.name)
            } else {
                format!("/{}", spec.name)
            };
            items.push(
                AutoFillItem::new(spec.usage, insert).with_detail(spec.description.to_string()),
            );
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(AutoFillMenu::new("Commands", 0, input.len(), items).with_selected(selected))
    }
}

/// `/model <...>` autocomplete, listing models from `~/.cass/models.json`.
pub fn model_autofill(
    input: &str,
    selected: usize,
    config: &Config,
) -> Result<Option<AutoFillMenu>> {
    let Some(rest) = input.strip_prefix("/model") else {
        return Ok(None);
    };
    if rest.is_empty() || !rest.chars().next().is_some_and(|c| c.is_whitespace()) {
        return Ok(None);
    }

    let arg = rest.trim_start_matches(char::is_whitespace);
    if arg.chars().any(char::is_whitespace) {
        return Ok(None);
    }
    let replacement_start = input.len() - arg.len();
    let query = arg.to_ascii_lowercase();

    let models = config::load_or_create_default_model_registry(&config.root)?;
    if !arg.is_empty() && models.models.iter().any(|model| model.id == arg) {
        return Ok(None);
    }

    let mut items = Vec::new();
    for model in models.models {
        if model_matches(&model, &query) {
            let id = model.id.clone();
            let detail = model_detail(&model, &config.model);
            items.push(AutoFillItem::new(id.clone(), id).with_detail(detail));
        }
    }

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            AutoFillMenu::new("Models", replacement_start, input.len(), items)
                .with_selected(selected),
        ))
    }
}

/// Resolve a model selection: find metadata, switch active provider when the
/// model belongs to a different provider, and update `config`. Does not
/// persist or touch reasoning effort; the caller does those steps.
pub fn apply_model_selection(config: &mut Config, model_id: &str) -> Result<()> {
    let models = config::load_or_create_default_model_registry(&config.root)?;
    let metadata = models
        .models
        .iter()
        .find(|model| model.id == model_id && model.provider == config.provider_id)
        .cloned()
        .or_else(|| {
            models
                .models
                .iter()
                .find(|model| model.id == model_id)
                .cloned()
        });

    if let Some(model) = &metadata {
        if model.provider != config.provider_id {
            let providers = config::load_or_create_default_provider_registry(&config.root)?;
            if let Some(provider) = providers
                .providers
                .iter()
                .find(|provider| provider.id == model.provider)
            {
                config.provider_id = provider.id.clone();
                config.active_provider = provider.to_resolved();
            }
        }
    }

    config.model = model_id.to_string();
    config.model_metadata = metadata;
    Ok(())
}

fn model_matches(model: &ModelDefinition, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    model.id.to_ascii_lowercase().contains(query)
        || model.provider.to_ascii_lowercase().contains(query)
        || model
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(query))
}

fn model_detail(model: &ModelDefinition, current_model: &str) -> String {
    let mut parts = Vec::new();
    if model.id == current_model {
        parts.push("current".to_string());
    }
    if let Some(name) = &model.display_name {
        if !name.trim().is_empty() && name != &model.id {
            parts.push(name.clone());
        }
    }
    parts.push(format!("provider {}", model.provider));
    if let Some(context_length) = model.context_length {
        parts.push(format!("ctx {context_length}"));
    }
    if let Some(max_output_tokens) = model.max_output_tokens {
        parts.push(format!("max {max_output_tokens}"));
    }
    if !model.supports_tools {
        parts.push("no tools".to_string());
    }
    if !model.supports_streaming {
        parts.push("no streaming".to_string());
    }
    if model.reasoning.supported {
        let label = if model.reasoning.required {
            format!("reasoning {} required", model.reasoning.default_effort)
        } else {
            format!("reasoning {}", model.reasoning.default_effort)
        };
        parts.push(label);
    } else {
        parts.push("no reasoning".to_string());
    }
    parts.join(" · ")
}

/// `/resume <...>` autocomplete, listing saved chats for the current cwd.
pub fn resume_chat_autofill(
    input: &str,
    selected: usize,
    config: &Config,
    cwd: &Path,
) -> Result<Option<AutoFillMenu>> {
    let Some(rest) = input.strip_prefix("/resume") else {
        return Ok(None);
    };
    if rest.is_empty() || !rest.chars().next().is_some_and(|c| c.is_whitespace()) {
        return Ok(None);
    }

    let arg = rest.trim_start_matches(char::is_whitespace);
    if arg.chars().any(char::is_whitespace) {
        return Ok(None);
    }
    let replacement_start = input.len() - arg.len();
    let query = arg.to_ascii_lowercase();

    let chats = conversation::list_chats(&config.conversations_dir(), cwd)?;
    if !arg.is_empty() && chats.iter().any(|chat| chat.id == arg) {
        return Ok(None);
    }

    let mut items = Vec::new();
    for chat in chats {
        if chat_matches(&chat, &query) {
            let detail = chat_detail(&chat);
            items.push(AutoFillItem::new(chat.id.clone(), chat.id).with_detail(detail));
        }
    }

    if items.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            AutoFillMenu::new("Chats", replacement_start, input.len(), items)
                .with_selected(selected),
        ))
    }
}

fn chat_matches(chat: &conversation::ChatSummary, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    chat.id.to_ascii_lowercase().contains(query)
        || chat.created_at.to_ascii_lowercase().contains(query)
        || chat.model.to_ascii_lowercase().contains(query)
        || chat.first_user_preview.to_ascii_lowercase().contains(query)
}

fn chat_detail(chat: &conversation::ChatSummary) -> String {
    let mut parts = vec![chat.created_at.clone(), short_model_name(&chat.model)];
    if !chat.first_user_preview.is_empty() {
        parts.push(chat.first_user_preview.clone());
    }
    parts.join(" · ")
}

fn short_model_name(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_string()
}

/// Apply a `/fast` subcommand: toggle/on/off persists the preference and
/// updates `config`; status reports the current state. Returns the
/// human-readable status message.
pub fn apply_fast_mode_command(config: &mut Config, command: FastModeCommand) -> Result<String> {
    match command {
        FastModeCommand::Status => Ok(fast_mode_status(&config.fast_mode_state())),
        FastModeCommand::Toggle | FastModeCommand::On | FastModeCommand::Off => {
            let enabled = match command {
                FastModeCommand::Toggle => !config.default_fast_mode,
                FastModeCommand::On => true,
                FastModeCommand::Off => false,
                FastModeCommand::Status => unreachable!(),
            };
            config::save_fast_mode_preference(&config.root, enabled)?;
            config.default_fast_mode = enabled;
            Ok(fast_mode_change_message(&config.fast_mode_state()))
        }
    }
}

pub fn fast_mode_change_message(state: &FastModeState) -> String {
    if state.active {
        "fast mode enabled".into()
    } else if state.preferred {
        format!(
            "fast mode preference on; unavailable for {}",
            state
                .unavailable_reason
                .as_deref()
                .unwrap_or("this provider/model")
        )
    } else {
        "fast mode off".into()
    }
}

pub fn fast_mode_status(state: &FastModeState) -> String {
    if state.active {
        "enabled".into()
    } else if state.preferred {
        format!(
            "preferred, unavailable for {}",
            state
                .unavailable_reason
                .as_deref()
                .unwrap_or("this provider/model")
        )
    } else {
        "off".into()
    }
}

pub fn model_status_message(config: &Config) -> String {
    let model = &config.model;
    let state = config.fast_mode_state();
    if state.active {
        format!("model: {model} · fast enabled")
    } else if state.preferred {
        format!("model: {model} · fast unavailable")
    } else {
        format!("model: {model}")
    }
}

/// Create a fresh conversation for the current cwd (`/new`).
pub fn create_new(config: &Config, cwd: &Path) -> Result<Conversation> {
    let global = fs::read_to_string(config.global_path()).ok();
    let base = prompt::build_base_system_prompt(global.as_deref());
    Conversation::create(&config.conversations_dir(), &config.model, cwd, base)
}

/// Load a saved conversation for `/resume`.
pub fn load_for_resume(config: &Config, id: &str) -> Result<(Conversation, Option<String>)> {
    Conversation::load(&config.conversations_dir(), id)
}

/// Build the `/status` summary string.
pub fn status_summary(
    chat_id: &str,
    config: &Config,
    access_mode: AccessMode,
    cwd: &Path,
    busy: bool,
    status: &str,
    record_count: usize,
) -> String {
    format!(
        "chat: {chat_id}\nstate: {}\nmodel: {}\nfast: {}\nmode: {access_mode}\ncwd: {}\nrecords: {record_count}\nstatus: {}",
        if busy { "running" } else { "idle" },
        config.model,
        fast_mode_status(&config.fast_mode_state()),
        cwd.display(),
        if status.is_empty() { "idle" } else { status }
    )
}

// --- Branch / login / logout facades ----------------------------------------

/// Load the branch family for the current conversation (`/branch`).
pub fn branch_family(config: &Config, conversation: &Conversation) -> Result<BranchFamily> {
    branch::load_family(&config.conversations_dir(), conversation)
}

/// Preview the tracked-file restore plan for a checkpoint.
pub fn preview_restore_plan(config: &Config, checkpoint: &Checkpoint) -> Result<String> {
    let plan =
        file_edits::plan_restore(&config.root, &checkpoint.chat_id, checkpoint.record_index)?;
    Ok(file_edits::summarize_plan(&plan))
}

/// Result of creating a branch from a checkpoint.
#[derive(Debug, Clone)]
pub struct BranchOutcome {
    pub conversation: Conversation,
    /// The chat id the branch was created from.
    pub source_chat_id: String,
    /// Human-readable status string for the transcript/status line.
    pub status: String,
    /// Present when files were restored; the file-restore transcript block
    /// content and whether it should be shown as an error (conflicts > 0).
    pub restore: Option<RestoreReport>,
}

/// File-restore report attached to a branch outcome.
#[derive(Debug, Clone)]
pub struct RestoreReport {
    pub summary: String,
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

impl RestoreReport {
    pub fn is_error(&self) -> bool {
        self.conflicts > 0
    }

    pub fn transcript_content(&self) -> String {
        format!(
            "{summary}\n\nApplied: {applied}\nSkipped: {skipped}\nConflicts: {conflicts}",
            summary = self.summary,
            applied = self.applied,
            skipped = self.skipped,
            conflicts = self.conflicts
        )
    }
}

/// Create a new branch from `checkpoint` in the current conversation family.
/// When `restore_files` is true, applies the tracked-file restore plan for
/// the checkpoint's source chat and record index.
pub fn create_branch_from_checkpoint(
    config: &Config,
    checkpoint: &Checkpoint,
    restore_files: bool,
) -> Result<BranchOutcome> {
    let (source, _) = Conversation::load(&config.conversations_dir(), &checkpoint.chat_id)?;
    let branch = branch::create_branch(&config.conversations_dir(), &source, checkpoint)?;
    let old_id = checkpoint.chat_id.clone();

    let mut restore_status = String::new();
    let mut restore = None;
    if restore_files {
        let plan =
            file_edits::plan_restore(&config.root, &checkpoint.chat_id, checkpoint.record_index)?;
        let summary = file_edits::summarize_plan(&plan);
        let outcome = file_edits::apply_restore_plan(&plan)?;
        restore_status = format!(
            "; restored files: {} applied, {} skipped, {} conflicts",
            outcome.applied, outcome.skipped, outcome.conflicts
        );
        restore = Some(RestoreReport {
            summary,
            applied: outcome.applied,
            skipped: outcome.skipped,
            conflicts: outcome.conflicts,
        });
    }

    let status = format!(
        "branched {} from {} at {}{}",
        branch.id,
        old_id,
        branch::checkpoint_title(checkpoint),
        restore_status
    );

    Ok(BranchOutcome {
        conversation: branch,
        source_chat_id: old_id,
        status,
        restore,
    })
}

/// List providers available for removal (`/logout` picker).
pub fn logout_candidates(root: &Path) -> Result<Vec<ProviderLogoutCandidate>> {
    setup::configured_providers(root)
}

/// Remove the given providers and their models, persisting the result.
pub fn remove_providers(root: &Path, provider_ids: &[String]) -> Result<LogoutResult> {
    setup::remove_providers(root, provider_ids)
}

/// Static built-in provider catalog for the login wizard.
pub fn login_catalog() -> Vec<ProviderCatalogEntry> {
    setup::provider_catalog()
}

/// Apply a completed login selection (upsert provider + model, set active).
pub fn apply_login(root: &Path, selections: &[SetupSelection], active_index: usize) -> Result<()> {
    setup::apply_setups(root, selections, active_index)
}

/// Discover models from a provider's `/models` endpoint.
pub async fn discover_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    setup::discover_models(base_url, api_key).await
}

/// Reload the active config from `root` (after login/logout changes).
pub fn reload_config(root: &Path) -> Result<Config> {
    Config::load_with_overrides(root.to_path_buf(), ConfigOverrides::default())
}

// --- Unified execution ------------------------------------------------------

/// Host-supplied context for [`execute`]. The host owns `config` and
/// `reasoning_effort`; `execute` mutates them for `/fast` and `/model`.
pub struct CommandContext<'a> {
    pub config: &'a mut Config,
    pub reasoning_effort: &'a mut ReasoningEffort,
    pub conversation: &'a Conversation,
    pub cwd: &'a Path,
    pub access_mode: AccessMode,
    pub busy: bool,
    pub status: &'a str,
}

/// The result of executing a slash command. The host maps each variant to its
/// own UI state updates (transcript block, status line, modal, etc.).
#[derive(Debug)]
pub enum CommandOutcome {
    /// Push a status block with this title/content and set the status line to
    /// the content.
    Status {
        title: &'static str,
        content: String,
    },
    /// `/new` succeeded; install the new conversation and reset view state.
    NewChat {
        conversation: Conversation,
        status: String,
    },
    /// `/resume` succeeded; install the conversation and rebuild the view.
    ResumedChat {
        conversation: Conversation,
        warning: Option<String>,
        status: String,
    },
    /// `/branch`: open the host's branch/restore picker over this family.
    OpenBranchPicker { family: BranchFamily },
    /// `/login`: open the host's provider-login wizard.
    OpenLoginWizard,
    /// `/logout`: open the host's provider-removal picker over these candidates.
    OpenLogoutPicker {
        candidates: Vec<ProviderLogoutCandidate>,
    },
    /// The command is not allowed while a turn is running; the string is the
    /// status-line message.
    Busy(String),
    /// A parse error (unknown command or bad usage); the host sets the status
    /// line only, without an error block, matching CLI behavior.
    ParseError(String),
    /// An execution error; the host sets the status line and pushes an error
    /// block with this title and message.
    Error {
        title: &'static str,
        message: String,
    },
}

/// Parse and execute a slash command. `input` must start with `/`.
///
/// This is the single entry point both hosts use. It performs the pure action
/// work for `/fast`, `/model`, `/new`, `/resume`, `/status` and returns
/// `Open*Picker` outcomes for the interactive commands. The host is
/// responsible for clearing the input, resetting view state, and recomputing
/// scroll/heights after handling the outcome.
pub fn execute(input: &str, ctx: &mut CommandContext) -> CommandOutcome {
    let command = match parse(input) {
        Ok(command) => command,
        Err(err) => return CommandOutcome::ParseError(err),
    };

    match command {
        LocalCommand::Branch => {
            if ctx.busy {
                return CommandOutcome::Busy("branch menu can be opened when idle".into());
            }
            match branch_family(ctx.config, ctx.conversation) {
                Ok(family) => CommandOutcome::OpenBranchPicker { family },
                Err(err) => CommandOutcome::Error {
                    title: "branch",
                    message: format!("branch menu failed: {err}"),
                },
            }
        }
        LocalCommand::Login => {
            if ctx.busy {
                return CommandOutcome::Busy("login can be opened when idle".into());
            }
            CommandOutcome::OpenLoginWizard
        }
        LocalCommand::Logout => {
            if ctx.busy {
                return CommandOutcome::Busy("logout can be opened when idle".into());
            }
            match logout_candidates(&ctx.config.root) {
                Ok(candidates) => CommandOutcome::OpenLogoutPicker { candidates },
                Err(err) => CommandOutcome::Error {
                    title: "logout",
                    message: format!("logout failed: {err}"),
                },
            }
        }
        LocalCommand::Fast(command) => {
            if ctx.busy {
                return CommandOutcome::Busy("fast mode can be changed when idle".into());
            }
            match apply_fast_mode_command(ctx.config, command) {
                Ok(message) => CommandOutcome::Status {
                    title: "fast",
                    content: message,
                },
                Err(err) => CommandOutcome::Error {
                    title: "fast",
                    message: format!("fast mode update failed: {err}"),
                },
            }
        }
        LocalCommand::Status => {
            let content = status_summary(
                &ctx.conversation.id,
                ctx.config,
                ctx.access_mode,
                ctx.cwd,
                ctx.busy,
                ctx.status,
                ctx.conversation.records.len(),
            );
            CommandOutcome::Status {
                title: "status",
                content,
            }
        }
        LocalCommand::Model(model) => {
            if ctx.busy {
                return CommandOutcome::Busy("model can be changed when idle".into());
            }
            match apply_model_selection(ctx.config, &model) {
                Ok(()) => {
                    *ctx.reasoning_effort =
                        ReasoningEffort::default_for_model(ctx.config.model_metadata.as_ref());
                    let _ = config::save_last_used_provider(
                        &ctx.config.root,
                        &ctx.config.provider_id,
                        &ctx.config.model,
                        *ctx.reasoning_effort,
                    );
                    CommandOutcome::Status {
                        title: "model",
                        content: model_status_message(ctx.config),
                    }
                }
                Err(err) => CommandOutcome::Error {
                    title: "model",
                    message: format!("model update failed: {err}"),
                },
            }
        }
        LocalCommand::New => {
            if ctx.busy {
                return CommandOutcome::Busy("new chat can be created when idle".into());
            }
            match create_new(ctx.config, ctx.cwd) {
                Ok(conversation) => {
                    let status = format!("new chat {}", conversation.id);
                    CommandOutcome::NewChat {
                        conversation,
                        status,
                    }
                }
                Err(err) => CommandOutcome::Error {
                    title: "new",
                    message: format!("new chat failed: {err}"),
                },
            }
        }
        LocalCommand::Resume(id) => {
            if ctx.busy {
                return CommandOutcome::Busy("chat can be resumed when idle".into());
            }
            match load_for_resume(ctx.config, &id) {
                Ok((conversation, warning)) => {
                    let status = format!("resumed chat {}", conversation.id);
                    CommandOutcome::ResumedChat {
                        conversation,
                        warning,
                        status,
                    }
                }
                Err(err) => CommandOutcome::Error {
                    title: "resume",
                    message: format!("resume failed: {err}"),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FastModeMetadata;
    use tempfile::tempdir;

    fn config_with_models(models_json: &str) -> (tempfile::TempDir, Config) {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("models.json"), models_json).unwrap();
        let config = Config {
            root: root.path().to_path_buf(),
            model: "alpha-model".to_string(),
            ..Config::default()
        };
        (root, config)
    }

    #[test]
    fn command_autofill_lists_new_command_and_hides_exact_match() {
        let menu = command_autofill("/n", 0).unwrap();

        assert_eq!(menu.items.len(), 1);
        assert_eq!(menu.items[0].label, "/new");
        assert_eq!(menu.items[0].insert, "/new");
        assert_eq!(menu.apply("/n").unwrap(), "/new");
        assert!(command_autofill("/new", 0).is_none());
    }

    #[test]
    fn command_autofill_lists_login_and_logout_commands() {
        let menu = command_autofill("/log", 0).unwrap();

        let labels = menu
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["/login", "/logout"]);
        assert_eq!(menu.items[0].insert, "/login");
        assert_eq!(menu.items[1].insert, "/logout");
    }

    #[test]
    fn parse_accepts_new_without_args() {
        assert_eq!(parse("/new").unwrap(), LocalCommand::New);
        assert_eq!(parse("/new extra"), Err("usage: /new".into()));
    }

    #[test]
    fn parse_accepts_branch_and_restore_alias() {
        assert_eq!(parse("/branch").unwrap(), LocalCommand::Branch);
        assert_eq!(parse("/restore").unwrap(), LocalCommand::Branch);
        assert_eq!(parse("/branch extra"), Err("usage: /branch".into()));
    }

    #[test]
    fn parse_accepts_login_and_logout_without_args() {
        assert_eq!(parse("/login").unwrap(), LocalCommand::Login);
        assert_eq!(parse("/logout").unwrap(), LocalCommand::Logout);
        assert_eq!(parse("/login extra"), Err("usage: /login".into()));
        assert_eq!(parse("/logout extra"), Err("usage: /logout".into()));
    }

    #[test]
    fn parse_accepts_fast_forms() {
        assert_eq!(
            parse("/fast").unwrap(),
            LocalCommand::Fast(FastModeCommand::Toggle)
        );
        assert_eq!(
            parse("/fast on").unwrap(),
            LocalCommand::Fast(FastModeCommand::On)
        );
        assert_eq!(
            parse("/fast off").unwrap(),
            LocalCommand::Fast(FastModeCommand::Off)
        );
        assert_eq!(
            parse("/fast status").unwrap(),
            LocalCommand::Fast(FastModeCommand::Status)
        );
        assert_eq!(
            parse("/fast maybe"),
            Err("usage: /fast [on|off|status]".into())
        );
    }

    #[test]
    fn parse_accepts_model_and_resume_with_args() {
        assert_eq!(
            parse("/model gpt-5").unwrap(),
            LocalCommand::Model("gpt-5".into())
        );
        assert_eq!(parse("/model"), Err("usage: /model <model>".into()));
        assert_eq!(
            parse("/resume chat-1").unwrap(),
            LocalCommand::Resume("chat-1".into())
        );
        assert_eq!(parse("/resume"), Err("usage: /resume <chat>".into()));
    }

    #[test]
    fn parse_rejects_unknown_command() {
        assert_eq!(parse("/nope"), Err("unknown command: /nope".into()));
    }

    #[test]
    fn fast_mode_status_distinguishes_preference_and_activation() {
        let mut config = Config {
            default_fast_mode: true,
            ..Config::default()
        };

        assert_eq!(
            fast_mode_status(&config.fast_mode_state()),
            "preferred, unavailable for provider fireworks"
        );

        config.provider_id = config::CHATGPT_CODEX_PROVIDER_ID.into();
        config.active_provider.kind = config::CHATGPT_CODEX_PROVIDER_KIND.into();
        config.model = config::CHATGPT_CODEX_DEFAULT_MODEL.into();
        config.model_metadata = Some(config::ModelDefinition {
            id: config::CHATGPT_CODEX_DEFAULT_MODEL.into(),
            provider: config::CHATGPT_CODEX_PROVIDER_ID.into(),
            display_name: None,
            context_length: None,
            max_output_tokens: None,
            supports_tools: true,
            supports_streaming: true,
            reasoning: Default::default(),
            fast_mode: FastModeMetadata { supported: true },
        });

        assert_eq!(fast_mode_status(&config.fast_mode_state()), "enabled");
        assert_eq!(
            model_status_message(&config),
            format!(
                "model: {} · fast enabled",
                config::CHATGPT_CODEX_DEFAULT_MODEL
            )
        );
    }

    #[test]
    fn model_autofill_lists_models_from_models_json() {
        let (_root, config) = config_with_models(
            r#"{
  "models": [
    {
      "id": "alpha-model",
      "provider": "fireworks",
      "display_name": "Alpha Model",
      "context_length": 1000,
      "max_output_tokens": 200
    },
    {
      "id": "beta-model",
      "provider": "other",
      "display_name": "Beta Model"
    }
  ]
}
"#,
        );

        let menu = model_autofill("/model ", 0, &config).unwrap().unwrap();

        assert_eq!(menu.items.len(), 2);
        assert_eq!(menu.items[0].label, "alpha-model");
        assert_eq!(menu.items[0].insert, "alpha-model");
        assert_eq!(menu.apply("/model ").unwrap(), "/model alpha-model");
        let detail = menu.items[0].detail.as_deref().unwrap();
        assert!(detail.contains("current"));
        assert!(detail.contains("Alpha Model"));
        assert!(detail.contains("provider fireworks"));
    }

    #[test]
    fn model_autofill_filters_and_hides_exact_matches() {
        let (_root, config) = config_with_models(
            r#"{
  "models": [
    { "id": "alpha-model", "provider": "fireworks" },
    { "id": "beta-model", "provider": "other", "display_name": "Beta Model" }
  ]
}
"#,
        );

        let menu = model_autofill("/model beta", 0, &config).unwrap().unwrap();
        assert_eq!(menu.items.len(), 1);
        assert_eq!(menu.items[0].insert, "beta-model");
        assert_eq!(menu.apply("/model beta").unwrap(), "/model beta-model");

        assert!(model_autofill("/model alpha-model", 0, &config)
            .unwrap()
            .is_none());
    }
}
