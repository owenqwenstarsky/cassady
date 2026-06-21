pub mod access;
pub mod agent;
pub mod app;
pub mod check;
pub mod cli;
pub mod config;
pub mod conversation;
pub mod docs;
pub mod error;
pub mod prompt;
pub mod providers;
pub mod tools;
pub mod ui;

pub async fn run() -> anyhow::Result<()> {
    app::run().await
}
