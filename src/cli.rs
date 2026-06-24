use clap::{Parser, Subcommand};
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
}

pub fn parse() -> Cli {
    Cli::parse()
}
