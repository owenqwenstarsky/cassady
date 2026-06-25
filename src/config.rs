use crate::access::AccessMode;
use crate::cli::Cli;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROVIDER_ID: &str = "fireworks";
pub const DEFAULT_PROVIDER_NAME: &str = "Fireworks";
pub const DEFAULT_PROVIDER_KIND: &str = "openai-compatible";
pub const CHATGPT_CODEX_PROVIDER_ID: &str = "chatgpt-codex";
pub const CHATGPT_CODEX_PROVIDER_NAME: &str = "ChatGPT Codex";
pub const CHATGPT_CODEX_PROVIDER_KIND: &str = "chatgpt-codex";
pub const CHATGPT_CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CHATGPT_CODEX_DEFAULT_MODEL: &str = "gpt-5.5";
pub const DEFAULT_MODEL: &str = "accounts/fireworks/models/qwen3p7-plus";
pub const DEFAULT_BASE_URL: &str = "https://api.fireworks.ai/inference/v1";
pub const DEFAULT_API_KEY_ENV: &str = "FIREWORKS_API_KEY";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reasoning_effort: Option<ReasoningEffort>,

    // Deprecated compatibility fields accepted from older config.json files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_access_mode: Option<AccessMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_message_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_tool_result_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_tool_result_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm_destructive_operations: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersFile {
    pub providers: Vec<ProviderDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDefinition {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsFile {
    pub models: Vec<ModelDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDefinition {
    pub id: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default = "default_true")]
    pub supports_tools: bool,
    #[serde(default = "default_true")]
    pub supports_streaming: bool,
    #[serde(default)]
    pub reasoning: ReasoningMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningMetadata {
    #[serde(default = "default_true")]
    pub supported: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "default_reasoning_effort")]
    pub default_effort: ReasoningEffort,
    #[serde(default)]
    pub request_format: ReasoningRequestFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Off,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningRequestFormat {
    ReasoningEffort,
    ReasoningObject,
}

#[derive(Debug, Clone)]
pub struct ResolvedProviderConfig {
    pub id: String,
    pub name: Option<String>,
    pub kind: String,
    pub base_url: String,
    /// Either a literal API key, an env-var reference like "$FIREWORKS_API_KEY", or empty for provider kinds that use external local auth.
    pub api_key: String,
    pub default_model: Option<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider_id: String,
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
    pub active_provider: ResolvedProviderConfig,
    pub model_metadata: Option<ModelDefinition>,
    pub default_access_mode: AccessMode,
    pub context_message_limit: usize,
    pub model_tool_result_limit: usize,
    pub ui_tool_result_limit: usize,
    pub show_reasoning: bool,
    pub confirm_destructive_operations: bool,
    pub root: PathBuf,
    pub docs_dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub access_mode: Option<AccessMode>,
}

impl ConfigOverrides {
    pub fn from_cli(cli: &Cli) -> Self {
        let access_mode = if cli.readonly {
            Some(AccessMode::ReadOnly)
        } else if cli.workspace_edit {
            Some(AccessMode::WorkspaceEdit)
        } else if cli.full_access {
            Some(AccessMode::FullAccess)
        } else {
            None
        };
        Self {
            model: cli.model.clone(),
            base_url: cli.base_url.clone(),
            api_key_env: cli.api_key_env.clone(),
            access_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyReference {
    Env(String),
    Literal,
}

impl Default for ReasoningMetadata {
    fn default() -> Self {
        Self {
            supported: true,
            required: false,
            default_effort: ReasoningEffort::Medium,
            request_format: ReasoningRequestFormat::ReasoningEffort,
        }
    }
}

impl Default for ReasoningRequestFormat {
    fn default() -> Self {
        Self::ReasoningEffort
    }
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ReasoningEffort::Off => "off",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        })
    }
}

impl ReasoningEffort {
    pub fn default_for_model(model: Option<&ModelDefinition>) -> Self {
        let Some(model) = model else {
            return Self::Off;
        };
        if !model.reasoning.supported {
            return Self::Off;
        }
        if model.reasoning.required && model.reasoning.default_effort == Self::Off {
            Self::Medium
        } else {
            model.reasoning.default_effort
        }
    }

    pub fn next_for_model(self, model: Option<&ModelDefinition>) -> Self {
        let Some(model) = model else {
            return Self::Off;
        };
        if !model.reasoning.supported {
            return Self::Off;
        }
        let required = model.reasoning.required;
        match (self, required) {
            (Self::Off, _) => Self::Low,
            (Self::Low, _) => Self::Medium,
            (Self::Medium, _) => Self::High,
            (Self::High, true) => Self::Low,
            (Self::High, false) => Self::Off,
        }
    }

    pub fn clamp_for_model(self, model: Option<&ModelDefinition>) -> Self {
        let Some(model) = model else {
            return Self::Off;
        };
        if !model.reasoning.supported {
            return Self::Off;
        }
        if model.reasoning.required && self == Self::Off {
            return Self::default_for_model(Some(model));
        }
        self
    }

    pub fn request_value(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Low => Some("low"),
            Self::Medium => Some("medium"),
            Self::High => Some("high"),
        }
    }
}

fn default_reasoning_effort() -> ReasoningEffort {
    ReasoningEffort::Medium
}

impl Default for Config {
    fn default() -> Self {
        let root = cass_root();
        let docs_dir = root.join("docs");
        let active_provider = default_provider_definition().into_resolved();
        Self {
            provider_id: DEFAULT_PROVIDER_ID.to_string(),
            model: DEFAULT_MODEL.to_string(),
            reasoning_effort: ReasoningEffort::Medium,
            active_provider,
            model_metadata: Some(default_model_definition()),
            default_access_mode: AccessMode::ReadOnly,
            context_message_limit: 80,
            model_tool_result_limit: 24_000,
            ui_tool_result_limit: 4_000,
            show_reasoning: false,
            confirm_destructive_operations: false,
            root,
            docs_dir,
        }
    }
}

pub fn cass_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cass")
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join("config.json")
}

pub fn providers_path(root: &Path) -> PathBuf {
    root.join("providers.json")
}

pub fn models_path(root: &Path) -> PathBuf {
    root.join("models.json")
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Self> {
        Self::load_with_overrides(cass_root(), ConfigOverrides::from_cli(cli))
    }

    pub fn load_with_overrides(root: PathBuf, overrides: ConfigOverrides) -> Result<Self> {
        fs::create_dir_all(root.join("conversations"))
            .with_context(|| format!("creating {}", root.join("conversations").display()))?;
        let docs_dir = crate::docs::install(&root)?;
        Self::load_from_root_with_docs_and_overrides(root, docs_dir, overrides)
    }

    pub fn load_from_root(root: PathBuf, cli: &Cli) -> Result<Self> {
        Self::load_with_overrides(root, ConfigOverrides::from_cli(cli))
    }

    pub fn load_from_root_with_docs(root: PathBuf, docs_dir: PathBuf, cli: &Cli) -> Result<Self> {
        Self::load_from_root_with_docs_and_overrides(root, docs_dir, ConfigOverrides::from_cli(cli))
    }

    pub fn load_from_root_with_docs_and_overrides(
        root: PathBuf,
        docs_dir: PathBuf,
        overrides: ConfigOverrides,
    ) -> Result<Self> {
        fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        let providers = load_or_create_default_provider_registry(&root)?;
        let models = load_or_create_default_model_registry(&root)?;
        let file = load_config_file(&root)?;

        let mut cfg = Config {
            root: root.clone(),
            docs_dir,
            ..Config::default()
        };

        if let Some(file) = &file {
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
            if let Some(v) = file.show_reasoning {
                cfg.show_reasoning = v;
            }
            if let Some(v) = file.confirm_destructive_operations {
                cfg.confirm_destructive_operations = v;
            }
        }

        if let Some(access_mode) = overrides.access_mode {
            cfg.default_access_mode = access_mode;
        }

        let requested_model = requested_model(file.as_ref(), &overrides);
        let provider_id_from_config = requested_provider_id(file.as_ref(), &providers);
        let legacy = legacy_provider_override(file.as_ref(), &overrides);

        let mut provider = resolve_provider(
            requested_model.as_deref().unwrap_or(DEFAULT_MODEL),
            provider_id_from_config.as_deref(),
            file.as_ref().and_then(|f| f.provider.as_deref()),
            legacy.as_ref(),
            &providers,
            &models,
        )?;

        if let Some(base_url) = &overrides.base_url {
            provider.base_url = base_url.clone();
        }
        if let Some(api_key_env) = &overrides.api_key_env {
            provider.api_key = format!("${api_key_env}");
        }

        let model = requested_model
            .or_else(|| provider.default_model.clone())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let metadata = find_model_for_provider(&models, &provider.id, &model).cloned();

        // Use the persisted reasoning effort if present, otherwise the model default.
        let reasoning_effort = file
            .as_ref()
            .and_then(|f| f.default_reasoning_effort)
            .map(|e| e.clamp_for_model(metadata.as_ref()))
            .unwrap_or_else(|| ReasoningEffort::default_for_model(metadata.as_ref()));

        cfg.provider_id = provider.id.clone();
        cfg.model = model;
        cfg.reasoning_effort = reasoning_effort;
        cfg.active_provider = provider;
        cfg.model_metadata = metadata;
        Ok(cfg)
    }

    pub fn conversations_dir(&self) -> PathBuf {
        self.root.join("conversations")
    }

    pub fn global_path(&self) -> PathBuf {
        self.root.join("global.md")
    }

    pub fn docs_dir(&self) -> PathBuf {
        self.docs_dir.clone()
    }

    pub fn resolved_api_key(&self) -> Result<String> {
        resolve_api_key(&self.active_provider.api_key)
    }

    pub fn ensure_provider_auth(&self) -> Result<()> {
        match self.active_provider.kind.as_str() {
            DEFAULT_PROVIDER_KIND => self.resolved_api_key().map(|_| ()),
            CHATGPT_CODEX_PROVIDER_KIND => crate::codex_auth::load_codex_access_token().map(|_| ()),
            kind => bail!("unsupported provider kind `{kind}`"),
        }
    }
}

impl ProviderDefinition {
    pub fn into_resolved(self) -> ResolvedProviderConfig {
        ResolvedProviderConfig {
            id: self.id,
            name: self.name,
            kind: self.kind,
            base_url: self.base_url,
            api_key: self.api_key,
            default_model: self.default_model,
            models: self.models,
        }
    }

    pub fn to_resolved(&self) -> ResolvedProviderConfig {
        self.clone().into_resolved()
    }
}

pub fn load_config_file(root: &Path) -> Result<Option<ConfigFile>> {
    let path = config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let file: ConfigFile =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(file))
}

/// Persist the last-used model and reasoning effort into `config.json` so the
/// next session starts with the same values.
pub fn save_last_used(root: &Path, model: &str, reasoning_effort: ReasoningEffort) -> Result<()> {
    let path = config_path(root);
    let mut file = load_config_file(root)?.unwrap_or_default();
    file.default_model = Some(model.to_string());
    file.default_reasoning_effort = Some(reasoning_effort);
    write_json_pretty(&path, &file)
}
pub fn load_or_create_default_provider_registry(root: &Path) -> Result<ProvidersFile> {
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    let path = providers_path(root);
    if !path.exists() {
        let file = ProvidersFile {
            providers: vec![default_provider_definition()],
        };
        write_json_pretty(&path, &file)?;
        return Ok(file);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn load_or_create_default_model_registry(root: &Path) -> Result<ModelsFile> {
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    let path = models_path(root);
    if !path.exists() {
        let file = ModelsFile {
            models: vec![default_model_definition()],
        };
        write_json_pretty(&path, &file)?;
        return Ok(file);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn default_provider_definition() -> ProviderDefinition {
    ProviderDefinition {
        id: DEFAULT_PROVIDER_ID.to_string(),
        name: Some(DEFAULT_PROVIDER_NAME.to_string()),
        kind: DEFAULT_PROVIDER_KIND.to_string(),
        base_url: DEFAULT_BASE_URL.to_string(),
        api_key: format!("${DEFAULT_API_KEY_ENV}"),
        default_model: Some(DEFAULT_MODEL.to_string()),
        models: vec![DEFAULT_MODEL.to_string()],
    }
}

pub fn default_model_definition() -> ModelDefinition {
    ModelDefinition {
        id: DEFAULT_MODEL.to_string(),
        provider: DEFAULT_PROVIDER_ID.to_string(),
        display_name: Some("Qwen 3p7 Plus".to_string()),
        context_length: Some(262_144),
        max_output_tokens: Some(32_768),
        supports_tools: true,
        supports_streaming: true,
        reasoning: ReasoningMetadata::default(),
    }
}

pub fn api_key_reference(spec: &str) -> Result<ApiKeyReference> {
    if let Some(name) = spec.strip_prefix('$') {
        if name.is_empty() {
            bail!("API key env-var reference must include a variable name, e.g. \"$FIREWORKS_API_KEY\"");
        }
        return Ok(ApiKeyReference::Env(name.to_string()));
    }
    if spec.is_empty() {
        bail!("literal API key must not be empty");
    }
    Ok(ApiKeyReference::Literal)
}

pub fn is_supported_provider_kind(kind: &str) -> bool {
    matches!(kind, DEFAULT_PROVIDER_KIND | CHATGPT_CODEX_PROVIDER_KIND)
}

pub fn resolve_api_key(spec: &str) -> Result<String> {
    match api_key_reference(spec)? {
        ApiKeyReference::Env(name) => {
            let value = std::env::var(&name)
                .with_context(|| format!("missing API key environment variable `{name}`"))?;
            if value.is_empty() {
                bail!("API key environment variable `{name}` is empty");
            }
            Ok(value)
        }
        ApiKeyReference::Literal => Ok(spec.to_string()),
    }
}

pub fn redact_api_key_for_display(spec: &str) -> String {
    match api_key_reference(spec) {
        Ok(ApiKeyReference::Env(name)) => format!("${name}"),
        Ok(ApiKeyReference::Literal) => "<literal API key>".to_string(),
        Err(_) => "<invalid API key reference>".to_string(),
    }
}

pub fn validate_registries(
    config_file: Option<&ConfigFile>,
    providers: &ProvidersFile,
    models: &ModelsFile,
) -> ValidationSummary {
    let mut out = ValidationSummary::default();
    let mut provider_ids = BTreeSet::new();
    let mut provider_counts = BTreeMap::<String, usize>::new();
    for provider in &providers.providers {
        *provider_counts.entry(provider.id.clone()).or_default() += 1;
        if provider.id.trim().is_empty() {
            out.errors
                .push("providers.json: provider id must not be empty".into());
        }
        if provider.kind.trim().is_empty() {
            out.errors.push(format!(
                "providers.json: provider `{}` kind must not be empty",
                provider.id
            ));
        } else if !is_supported_provider_kind(&provider.kind) {
            out.errors.push(format!(
                "providers.json: provider `{}` uses unsupported kind `{}`",
                provider.id, provider.kind
            ));
        }
        if provider.base_url.trim().is_empty() {
            out.errors.push(format!(
                "providers.json: provider `{}` base_url must not be empty",
                provider.id
            ));
        } else if reqwest::Url::parse(&provider.base_url).is_err() {
            out.errors.push(format!(
                "providers.json: provider `{}` base_url is not a valid URL",
                provider.id
            ));
        }
        if provider.kind == DEFAULT_PROVIDER_KIND {
            if let Err(err) = api_key_reference(&provider.api_key) {
                out.errors.push(format!(
                    "providers.json: provider `{}` has invalid api_key: {err}",
                    provider.id
                ));
            }
        } else if provider.kind == CHATGPT_CODEX_PROVIDER_KIND
            && !provider.api_key.trim().is_empty()
        {
            out.warnings.push(format!(
                "providers.json: provider `{}` ignores api_key because ChatGPT Codex uses local Codex auth",
                provider.id
            ));
        }
        if let Some(model) = &provider.default_model {
            if model.trim().is_empty() {
                out.errors.push(format!(
                    "providers.json: provider `{}` default_model must not be empty",
                    provider.id
                ));
            }
        }
        for model in &provider.models {
            if model.trim().is_empty() {
                out.errors.push(format!(
                    "providers.json: provider `{}` models entries must not be empty",
                    provider.id
                ));
            }
        }
        provider_ids.insert(provider.id.clone());
    }
    for (id, count) in provider_counts {
        if count > 1 {
            out.errors
                .push(format!("providers.json: duplicate provider id `{id}`"));
        }
    }

    let mut model_counts = BTreeMap::<(String, String), usize>::new();
    for model in &models.models {
        *model_counts
            .entry((model.provider.clone(), model.id.clone()))
            .or_default() += 1;
        if model.id.trim().is_empty() {
            out.errors
                .push("models.json: model id must not be empty".into());
        }
        if model.provider.trim().is_empty() {
            out.errors.push(format!(
                "models.json: model `{}` provider must not be empty",
                model.id
            ));
        } else if !provider_ids.contains(&model.provider) {
            out.errors.push(format!(
                "models.json: model `{}` references unknown provider `{}`",
                model.id, model.provider
            ));
        }
        if matches!(model.context_length, Some(0)) {
            out.errors.push(format!(
                "models.json: model `{}` context_length must be positive",
                model.id
            ));
        }
        if matches!(model.max_output_tokens, Some(0)) {
            out.errors.push(format!(
                "models.json: model `{}` max_output_tokens must be positive",
                model.id
            ));
        }
        if model.reasoning.required && !model.reasoning.supported {
            out.errors.push(format!(
                "models.json: model `{}` cannot require reasoning when reasoning is unsupported",
                model.id
            ));
        }
        if model.reasoning.required && model.reasoning.default_effort == ReasoningEffort::Off {
            out.errors.push(format!(
                "models.json: model `{}` reasoning default_effort cannot be `off` when reasoning is required",
                model.id
            ));
        }
    }
    for ((provider, id), count) in model_counts {
        if count > 1 {
            out.errors.push(format!(
                "models.json: duplicate model `{id}` for provider `{provider}`"
            ));
        }
    }

    for provider in &providers.providers {
        if let Some(default_model) = &provider.default_model {
            if find_model_for_provider(models, &provider.id, default_model).is_none() {
                out.warnings.push(format!(
                    "providers.json: provider `{}` default_model `{}` has no matching models.json entry",
                    provider.id, default_model
                ));
            }
        }
        for model in &provider.models {
            if find_model_for_provider(models, &provider.id, model).is_none() {
                out.warnings.push(format!(
                    "providers.json: provider `{}` model `{}` has no matching models.json entry",
                    provider.id, model
                ));
            }
        }
    }

    if let Some(file) = config_file {
        if file.provider.is_some() {
            out.warnings.push(
                "config.json: `provider` is deprecated; use `default_provider` or infer provider from `default_model`".into(),
            );
        }
        if file.model.is_some() {
            out.warnings
                .push("config.json: `model` is deprecated; use `default_model`".into());
        }
        if file.base_url.is_some() || file.api_key_env.is_some() {
            out.warnings.push(
                "config.json: `base_url` and `api_key_env` are deprecated; move provider connection details to providers.json".into(),
            );
        }
        if let Some(default_provider) = &file.default_provider {
            if !provider_ids.contains(default_provider) {
                out.errors.push(format!(
                    "config.json: default_provider `{default_provider}` does not exist in providers.json"
                ));
            }
        }
    }

    out
}

#[derive(Debug, Default, Clone)]
pub struct ValidationSummary {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn find_model_for_provider<'a>(
    models: &'a ModelsFile,
    provider_id: &str,
    model_id: &str,
) -> Option<&'a ModelDefinition> {
    models
        .models
        .iter()
        .find(|m| m.provider == provider_id && m.id == model_id)
}

fn requested_model(file: Option<&ConfigFile>, overrides: &ConfigOverrides) -> Option<String> {
    overrides.model.clone().or_else(|| {
        file.and_then(|f| {
            f.default_model
                .clone()
                .or_else(|| f.model.clone())
                .filter(|m| !m.trim().is_empty())
        })
    })
}

fn requested_provider_id(file: Option<&ConfigFile>, providers: &ProvidersFile) -> Option<String> {
    let file = file?;
    if let Some(default_provider) = &file.default_provider {
        return Some(default_provider.clone());
    }
    let legacy_provider = file.provider.as_ref()?;
    if providers.providers.iter().any(|p| p.id == *legacy_provider) {
        Some(legacy_provider.clone())
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct LegacyProviderOverride {
    base_url: Option<String>,
    api_key: Option<String>,
}

fn legacy_provider_override(
    file: Option<&ConfigFile>,
    overrides: &ConfigOverrides,
) -> Option<LegacyProviderOverride> {
    let base_url = overrides
        .base_url
        .clone()
        .or_else(|| file.and_then(|f| f.base_url.clone()));
    let api_key = overrides
        .api_key_env
        .as_ref()
        .map(|env| format!("${env}"))
        .or_else(|| file.and_then(|f| f.api_key_env.as_ref().map(|env| format!("${env}"))));
    if base_url.is_some() || api_key.is_some() {
        Some(LegacyProviderOverride { base_url, api_key })
    } else {
        None
    }
}

fn resolve_provider(
    model: &str,
    explicit_provider_id: Option<&str>,
    legacy_provider_name: Option<&str>,
    legacy: Option<&LegacyProviderOverride>,
    providers: &ProvidersFile,
    models: &ModelsFile,
) -> Result<ResolvedProviderConfig> {
    if let Some(id) = explicit_provider_id {
        if let Some(provider) = providers.providers.iter().find(|p| p.id == id) {
            return Ok(provider.to_resolved());
        }
        if let Some(legacy) = legacy {
            return Ok(legacy_resolved_provider(id, legacy));
        }
        bail!("configured provider `{id}` does not exist in providers.json");
    }

    if let Some(legacy) = legacy {
        let id = legacy_provider_name.unwrap_or("legacy-openai-compatible");
        return Ok(legacy_resolved_provider(id, legacy));
    }

    if let Some(provider_id) = unique_provider_for_model(model, providers, models)? {
        if let Some(provider) = providers.providers.iter().find(|p| p.id == provider_id) {
            return Ok(provider.to_resolved());
        }
        bail!("model `{model}` references unknown provider `{provider_id}` in models.json");
    }

    if let Some(provider) = providers
        .providers
        .iter()
        .find(|p| p.id == DEFAULT_PROVIDER_ID)
    {
        return Ok(provider.to_resolved());
    }

    if providers.providers.len() == 1 {
        return Ok(providers.providers[0].to_resolved());
    }

    bail!("could not resolve active provider; set config.json `default_provider` or choose a model with a unique provider in models.json")
}

fn unique_provider_for_model(
    model: &str,
    providers: &ProvidersFile,
    models: &ModelsFile,
) -> Result<Option<String>> {
    let mut ids = BTreeSet::new();
    for model_def in &models.models {
        if model_def.id == model {
            ids.insert(model_def.provider.clone());
        }
    }
    for provider in &providers.providers {
        if provider.default_model.as_deref() == Some(model)
            || provider.models.iter().any(|candidate| candidate == model)
        {
            ids.insert(provider.id.clone());
        }
    }
    match ids.len() {
        0 => Ok(None),
        1 => Ok(ids.into_iter().next()),
        _ => bail!(
            "model `{model}` is configured for multiple providers; set config.json `default_provider`"
        ),
    }
}

fn legacy_resolved_provider(id: &str, legacy: &LegacyProviderOverride) -> ResolvedProviderConfig {
    ResolvedProviderConfig {
        id: id.to_string(),
        name: Some("Legacy OpenAI-compatible provider".to_string()),
        kind: DEFAULT_PROVIDER_KIND.to_string(),
        base_url: legacy
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        api_key: legacy
            .api_key
            .clone()
            .unwrap_or_else(|| format!("${DEFAULT_API_KEY_ENV}")),
        default_model: Some(DEFAULT_MODEL.to_string()),
        models: vec![DEFAULT_MODEL.to_string()],
    }
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n")).with_context(|| format!("writing {}", path.display()))
}

fn default_true() -> bool {
    true
}
