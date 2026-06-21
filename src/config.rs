use crate::access::AccessMode;
use crate::cli::Cli;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub default_access_mode: Option<AccessMode>,
    pub context_message_limit: Option<usize>,
    pub model_tool_result_limit: Option<usize>,
    pub ui_tool_result_limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
    pub default_access_mode: AccessMode,
    pub context_message_limit: usize,
    pub model_tool_result_limit: usize,
    pub ui_tool_result_limit: usize,
    pub root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "openai-compatible".to_string(),
            model: "accounts/fireworks/models/qwen3p7-plus".to_string(),
            base_url: "https://api.fireworks.ai/inference/v1".to_string(),
            api_key_env: "FIREWORKS_API_KEY".to_string(),
            default_access_mode: AccessMode::ReadOnly,
            context_message_limit: 80,
            model_tool_result_limit: 24_000,
            ui_tool_result_limit: 4_000,
            root: cass_root(),
        }
    }
}

pub fn cass_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cass")
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Self> {
        let root = cass_root();
        fs::create_dir_all(root.join("conversations"))
            .with_context(|| format!("creating {}", root.join("conversations").display()))?;
        let mut cfg = Config::default();
        cfg.root = root.clone();

        let path = root.join("config.json");
        if path.exists() {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let file: ConfigFile = serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            if let Some(v) = file.provider {
                cfg.provider = v;
            }
            if let Some(v) = file.model {
                cfg.model = v;
            }
            if let Some(v) = file.base_url {
                cfg.base_url = v;
            }
            if let Some(v) = file.api_key_env {
                cfg.api_key_env = v;
            }
            if let Some(v) = file.default_access_mode {
                cfg.default_access_mode = v;
            }
            if let Some(v) = file.context_message_limit {
                cfg.context_message_limit = v;
            }
            if let Some(v) = file.model_tool_result_limit {
                cfg.model_tool_result_limit = v;
            }
            if let Some(v) = file.ui_tool_result_limit {
                cfg.ui_tool_result_limit = v;
            }
        }

        if let Some(v) = &cli.model {
            cfg.model = v.clone();
        }
        if let Some(v) = &cli.base_url {
            cfg.base_url = v.clone();
        }
        if let Some(v) = &cli.api_key_env {
            cfg.api_key_env = v.clone();
        }
        if cli.readonly {
            cfg.default_access_mode = AccessMode::ReadOnly;
        }
        if cli.full_access {
            cfg.default_access_mode = AccessMode::FullAccess;
        }

        Ok(cfg)
    }

    pub fn conversations_dir(&self) -> PathBuf {
        self.root.join("conversations")
    }

    pub fn global_path(&self) -> PathBuf {
        self.root.join("global.md")
    }
}
