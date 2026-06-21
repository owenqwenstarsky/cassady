use thiserror::Error;

#[derive(Debug, Error)]
pub enum CassError {
    #[error("tool `{tool}` is unavailable in {mode} mode")]
    ToolUnavailable { tool: String, mode: String },

    #[error("path escapes read-only root: {path}")]
    PathEscapesRoot { path: String },

    #[error("invalid arguments for {tool}: {message}")]
    InvalidToolArgs { tool: String, message: String },
}
