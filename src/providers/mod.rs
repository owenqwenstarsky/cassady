pub mod chatgpt_codex;
pub mod openai_compatible;
pub mod types;

use crate::agent::AgentEvent;
use crate::config::{Config, ReasoningEffort, CHATGPT_CODEX_PROVIDER_KIND, DEFAULT_PROVIDER_KIND};
use crate::providers::chatgpt_codex::{ChatGptCodexProvider, ChatGptCodexSettings};
use crate::providers::openai_compatible::{OpenAiCompatibleProvider, OpenAiCompatibleSettings};
use crate::providers::types::{CompletionResult, ModelMessage};
use crate::tools::ToolSpec;
use anyhow::{bail, Result};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ProviderClient {
    OpenAiCompatible(OpenAiCompatibleProvider),
    ChatGptCodex(ChatGptCodexProvider),
}

impl ProviderClient {
    pub fn from_config(config: &Config, reasoning_effort: ReasoningEffort) -> Result<Self> {
        match config.active_provider.kind.as_str() {
            DEFAULT_PROVIDER_KIND => {
                let api_key = config.resolved_api_key()?;
                let reasoning_request_format = config
                    .model_metadata
                    .as_ref()
                    .map(|model| model.reasoning.request_format)
                    .unwrap_or_default();
                Ok(Self::OpenAiCompatible(OpenAiCompatibleProvider::new(
                    OpenAiCompatibleSettings {
                        model: config.model.clone(),
                        base_url: config.active_provider.base_url.clone(),
                        api_key,
                        reasoning_effort,
                        reasoning_request_format,
                    },
                )))
            }
            CHATGPT_CODEX_PROVIDER_KIND => Ok(Self::ChatGptCodex(ChatGptCodexProvider::new(
                ChatGptCodexSettings {
                    model: config.model.clone(),
                    endpoint: config.active_provider.base_url.clone(),
                    reasoning_effort,
                },
            ))),
            kind => bail!("unsupported provider kind `{kind}`"),
        }
    }

    pub async fn complete(
        &self,
        messages: Vec<ModelMessage>,
        tools: Vec<ToolSpec>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<CompletionResult> {
        match self {
            Self::OpenAiCompatible(provider) => provider.complete(messages, tools, tx).await,
            Self::ChatGptCodex(provider) => provider.complete(messages, tools, tx).await,
        }
    }
}
