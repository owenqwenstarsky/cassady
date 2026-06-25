pub mod access;
pub mod agent;
pub mod app;
pub mod branch;
pub mod check;
pub mod cli;
pub mod codex_auth;
pub mod config;
pub mod conversation;
pub mod docs;
pub mod embedding;
pub mod error;
pub mod file_edits;
pub mod menu;
pub mod prelude;
pub mod prompt;
pub mod providers;
pub mod security;
pub mod setup;
pub mod tools;
pub mod ui;
pub mod update;

pub async fn run() -> anyhow::Result<()> {
    app::run().await
}
