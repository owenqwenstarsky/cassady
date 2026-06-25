use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
#[command(name = "cass", version, about = "Cassady/Cass terminal coding agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Resume a chat. Without a chat id, list chats for the current cwd.
    #[arg(long, num_args = 0..=1, value_name = "CHAT_ID")]
    pub resume: Option<Option<String>>,

    /// Override model for this session.
    #[arg(long)]
    pub model: Option<String>,

    /// Override OpenAI-compatible base URL.
    #[arg(long)]
    pub base_url: Option<String>,

    /// Override API key environment variable name.
    #[arg(long)]
    pub api_key_env: Option<String>,

    /// Set launch cwd/root for relative paths.
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Force read-only mode.
    #[arg(long, conflicts_with_all = ["workspace_edit", "full_access"])]
    pub readonly: bool,

    /// Force workspace-edit mode: edit workspace files directly, ask before shell.
    #[arg(long, conflicts_with_all = ["readonly", "full_access"])]
    pub workspace_edit: bool,

    /// Force full-access mode.
    #[arg(long, conflicts_with_all = ["readonly", "workspace_edit"])]
    pub full_access: bool,
}

#[derive(Debug, Subcommand, Clone, PartialEq, Eq)]
pub enum Command {
    /// Validate Cass config files.
    Check,
    /// Configure or update OpenAI-compatible provider login settings.
    Login,
    /// Remove saved providers and their models.
    Logout,
    /// Configure an OpenAI-compatible provider and first model.
    Setup,
    /// Update Cassady from official GitHub releases.
    Update(UpdateArgs),
}

#[derive(Debug, Args, Clone, PartialEq, Eq)]
pub struct UpdateArgs {
    /// Check the latest release without installing.
    #[arg(long)]
    pub check: bool,

    /// Show what would be updated without downloading or installing.
    #[arg(long)]
    pub dry_run: bool,

    /// Accept default prompts for non-interactive use.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Require a matching prebuilt archive and do not fall back to source.
    #[arg(long, conflicts_with = "source")]
    pub prebuilt: bool,

    /// Build from release source even when a prebuilt archive exists.
    #[arg(long, conflicts_with = "prebuilt")]
    pub source: bool,

    /// Install a specific release tag, such as v0.2.7.
    #[arg(long, value_name = "TAG")]
    pub to: Option<String>,
}

pub fn parse() -> Cli {
    Cli::parse()
}
