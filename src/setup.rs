use crate::check;
use crate::cli::Cli;
use crate::config::{
    self, ConfigFile, ModelDefinition, ModelsFile, ProviderDefinition, ProvidersFile,
    ReasoningEffort, ReasoningMetadata, ReasoningRequestFormat, DEFAULT_PROVIDER_KIND,
};
use crate::menu::{Menu, MenuItem, TextPrompt};
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupMode {
    Explicit,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupOutcome {
    pub start_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCatalogEntry {
    pub name: &'static str,
    pub id: &'static str,
    pub base_url: &'static str,
    pub api_key_env: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupSelection {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key_env: String,
    pub model_id: String,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelItem>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
}

fn print_banner() {
    println!("Cassady setup");
    println!("Configure an OpenAI-compatible provider, API key environment variable, and model.");
}

fn section(title: &str) {
    println!("\n{title}");
    println!("{}", "─".repeat(title.chars().count().max(12).min(72)));
}

fn key_value(label: &str, value: &str) {
    println!("  {label:<12} {value}");
}

fn info(message: impl AsRef<str>) {
    println!("  → {}", message.as_ref());
}

fn success(message: impl AsRef<str>) {
    println!("  ✓ {}", message.as_ref());
}

fn warn(message: impl AsRef<str>) {
    print_wrapped("  ! ", message.as_ref());
}

fn hint(message: impl AsRef<str>) {
    print_wrapped("    ", message.as_ref());
}

fn print_wrapped(prefix: &str, text: &str) {
    let width = crossterm::terminal::size()
        .map(|(width, _)| width as usize)
        .unwrap_or(100)
        .clamp(40, 140);
    let available = width.saturating_sub(prefix.chars().count()).max(20);

    for line in text.lines() {
        let mut current = String::new();
        for word in line.split_whitespace() {
            let next_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };
            if next_len > available && !current.is_empty() {
                println!("{prefix}{current}");
                current.clear();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if current.is_empty() {
            println!("{prefix}");
        } else {
            println!("{prefix}{current}");
        }
    }
}

pub fn provider_catalog() -> Vec<ProviderCatalogEntry> {
    vec![
        ProviderCatalogEntry {
            name: "OpenAI",
            id: "openai",
            base_url: "https://api.openai.com/v1",
            api_key_env: "OPENAI_API_KEY",
        },
        ProviderCatalogEntry {
            name: "xAI",
            id: "xai",
            base_url: "https://api.x.ai/v1",
            api_key_env: "XAI_API_KEY",
        },
        ProviderCatalogEntry {
            name: "Fireworks",
            id: "fireworks",
            base_url: "https://api.fireworks.ai/inference/v1",
            api_key_env: "FIREWORKS_API_KEY",
        },
        ProviderCatalogEntry {
            name: "Groq",
            id: "groq",
            base_url: "https://api.groq.com/openai/v1",
            api_key_env: "GROQ_API_KEY",
        },
        ProviderCatalogEntry {
            name: "OpenRouter",
            id: "openrouter",
            base_url: "https://openrouter.ai/api/v1",
            api_key_env: "OPENROUTER_API_KEY",
        },
        ProviderCatalogEntry {
            name: "OpenCode Zen",
            id: "opencode-zen",
            base_url: "https://opencode.ai/zen/v1",
            api_key_env: "OPENCODE_API_KEY",
        },
        ProviderCatalogEntry {
            name: "OpenCode Go",
            id: "opencode-go",
            base_url: "https://opencode.ai/zen/go/v1",
            api_key_env: "OPENCODE_API_KEY",
        },
        ProviderCatalogEntry {
            name: "Cerebras",
            id: "cerebras",
            base_url: "https://api.cerebras.ai/v1",
            api_key_env: "CEREBRAS_API_KEY",
        },
        ProviderCatalogEntry {
            name: "Novita",
            id: "novita",
            base_url: "https://api.novita.ai/v3/openai",
            api_key_env: "NOVITA_API_KEY",
        },
        ProviderCatalogEntry {
            name: "Together",
            id: "together",
            base_url: "https://api.together.xyz/v1",
            api_key_env: "TOGETHER_API_KEY",
        },
    ]
}

pub async fn run(cli: &Cli, mode: SetupMode) -> Result<SetupOutcome> {
    let root = config::cass_root();
    fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

    if !io::stdin().is_terminal() {
        bail!("setup is interactive; run `cass setup` in a terminal");
    }

    print_banner();

    match mode {
        SetupMode::Explicit => {
            if existing_setup_files(&root)
                && !ask_yes_no(
                    "Update your active provider/model while preserving unrelated entries?",
                    false,
                )?
            {
                println!("Setup cancelled.");
                return Ok(SetupOutcome {
                    start_session: false,
                });
            }
        }
        SetupMode::Auto => {
            println!();
            hint("Cassady needs this before starting your first chat.");
            if !ask_yes_no("Start setup now?", true)? {
                println!("Run `cass setup` when you are ready.");
                return Ok(SetupOutcome {
                    start_session: false,
                });
            }
        }
    }

    let providers = choose_providers()?;
    let total_providers = providers.len();
    let mut selections = Vec::new();

    for (idx, provider) in providers.into_iter().enumerate() {
        let configured = configure_provider(provider, idx + 1, total_providers).await?;
        selections.push(configured.selection);
    }

    let active_index = choose_active_provider(&selections)?;
    let active_api_key_env = selections[active_index].api_key_env.clone();
    apply_setups(&root, &selections, active_index)?;

    let report = check::run(cli)?;
    if report.has_errors() {
        if std::env::var(&active_api_key_env).is_err() {
            section("Setup saved");
            warn("Your active provider API key is not available in this shell.");
            hint(format!("Set it with: export {active_api_key_env}=..."));
            hint("Then run: cass");
        } else {
            section("Setup saved with issues");
            print!("{}", report.render());
            hint("Run `cass setup` to try again or edit ~/.cass/config.json manually.");
        }
        return Ok(SetupOutcome {
            start_session: false,
        });
    }

    section("Setup complete");
    success("Configuration saved and validated");
    info("Starting your first Cassady session…");
    Ok(SetupOutcome {
        start_session: true,
    })
}

#[derive(Debug, Clone)]
struct ChosenProvider {
    name: String,
    id: String,
    base_url: String,
    api_key_env: String,
}

fn choose_providers() -> Result<Vec<ChosenProvider>> {
    let catalog = provider_catalog();
    let mut items: Vec<MenuItem> = catalog
        .iter()
        .map(|entry| MenuItem::with_detail(entry.name, entry.base_url))
        .collect();
    items.push(MenuItem::with_detail(
        "Custom OpenAI-compatible provider",
        "enter your own base URL",
    ));

    let selected = Menu::new("Choose the providers you want to configure", items)
        .select_many(&BTreeSet::new(), true)?;

    let mut providers = Vec::new();
    for idx in selected {
        if idx < catalog.len() {
            let entry = &catalog[idx];
            providers.push(ChosenProvider {
                name: entry.name.to_string(),
                id: entry.id.to_string(),
                base_url: entry.base_url.to_string(),
                api_key_env: entry.api_key_env.to_string(),
            });
        } else {
            providers.push(choose_custom_provider()?);
            while choose_bool("Add another custom provider?", false)? {
                providers.push(choose_custom_provider()?);
            }
        }
    }

    let names = providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    success(format!("Providers selected: {names}"));
    Ok(providers)
}

fn choose_custom_provider() -> Result<ChosenProvider> {
    section("Custom provider");
    let name = ask_required("Provider name")?;
    let id = loop {
        let value = ask_required("Provider id")?;
        match validate_provider_id(&value) {
            Ok(()) => break value,
            Err(err) => warn(err.to_string()),
        }
    };
    let base_url = loop {
        let value = ask_required("Base URL")?;
        match validate_base_url(&value) {
            Ok(()) => break value,
            Err(err) => warn(err.to_string()),
        }
    };
    let api_key_env = ask_required("API key environment variable")?;
    Ok(ChosenProvider {
        name,
        id,
        base_url,
        api_key_env,
    })
}

struct ConfiguredProvider {
    selection: SetupSelection,
}

async fn configure_provider(
    provider: ChosenProvider,
    index: usize,
    total: usize,
) -> Result<ConfiguredProvider> {
    section(&format!("Provider {index}/{total}: {}", provider.name));
    key_value("id", &provider.id);
    key_value("endpoint", &provider.base_url);

    let api_key_env = ask_default("API key environment variable", &provider.api_key_env)?;
    if !looks_like_env_var(&api_key_env) {
        warn(format!(
            "`{api_key_env}` is an unusual environment variable name. Continuing."
        ));
    }
    let api_key = match std::env::var(&api_key_env) {
        Ok(value) if !value.is_empty() => {
            success(format!("{api_key_env} is set"));
            Some(value)
        }
        _ => {
            warn(format!(
                "{api_key_env} is not set in this shell. Setup can still be saved."
            ));
            hint(format!("Later, run: export {api_key_env}=..."));
            None
        }
    };

    let model_id = choose_model(&provider, api_key.as_deref()).await?;

    section("Model capabilities");
    key_value("model", &model_id);
    let supports_tools = choose_bool("Does this model support tool calls?", true)?;
    if supports_tools {
        success("Tool calls enabled");
    } else {
        warn("Cassady works best with models that support tool calls.");
    }
    let supports_reasoning =
        choose_bool("Does this model support reasoning effort controls?", true)?;
    if supports_reasoning {
        success("Reasoning controls enabled");
    }

    Ok(ConfiguredProvider {
        selection: SetupSelection {
            provider_id: provider.id,
            provider_name: provider.name,
            base_url: provider.base_url,
            api_key_env,
            model_id,
            supports_tools,
            supports_reasoning,
        },
    })
}

fn choose_active_provider(selections: &[SetupSelection]) -> Result<usize> {
    if selections.is_empty() {
        bail!("no providers selected");
    }
    if selections.len() == 1 {
        success(format!(
            "Active provider: {} ({})",
            selections[0].provider_name, selections[0].model_id
        ));
        return Ok(0);
    }
    let items = selections
        .iter()
        .map(|selection| {
            MenuItem::with_detail(
                selection.provider_name.clone(),
                format!("{} · {}", selection.provider_id, selection.model_id),
            )
        })
        .collect();
    let choice = Menu::new("Which provider should Cass use first?", items).select_one(0)?;
    success(format!(
        "Active provider: {} ({})",
        selections[choice].provider_name, selections[choice].model_id
    ));
    Ok(choice)
}

async fn choose_model(provider: &ChosenProvider, api_key: Option<&str>) -> Result<String> {
    section("Model");

    let Some(api_key) = api_key else {
        warn("Model discovery was skipped because the API key is not available in this shell.");
        hint("Enter the model id manually now; Cassady will use it after the key is exported.");
        return ask_manual_model();
    };

    loop {
        info(format!("Fetching models from {}…", provider.name));
        match discover_models(&provider.base_url, api_key).await {
            Ok(models) if !models.is_empty() => return choose_discovered_model(models),
            Ok(_) => {
                warn(format!("{} returned an empty model list.", provider.name));
            }
            Err(err) => {
                warn(format!("Could not fetch models from {}.", provider.name));
                hint(err.to_string());
            }
        }

        match choose_model_discovery_fallback()? {
            ModelDiscoveryFallback::Retry => continue,
            ModelDiscoveryFallback::Manual => return ask_manual_model(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelDiscoveryFallback {
    Retry,
    Manual,
}

fn choose_model_discovery_fallback() -> Result<ModelDiscoveryFallback> {
    let choice = Menu::new(
        "Model list could not be loaded. What would you like to do?",
        vec![
            MenuItem::with_detail("Retry model discovery", "try GET /models again"),
            MenuItem::with_detail(
                "Enter model id manually",
                "continue without provider model list",
            ),
        ],
    )
    .select_one(0)?;

    Ok(match choice {
        0 => ModelDiscoveryFallback::Retry,
        _ => ModelDiscoveryFallback::Manual,
    })
}

fn choose_discovered_model(mut discovered: Vec<String>) -> Result<String> {
    success(format!("Found {} models", discovered.len()));
    discovered.sort();
    discovered.dedup();
    let mut items: Vec<MenuItem> = discovered
        .iter()
        .map(|model| MenuItem::new(model.clone()))
        .collect();
    items.push(MenuItem::with_detail(
        "Enter model id manually",
        "use this if the model is not listed",
    ));

    let choice = Menu::new("Choose your first model", items)
        .with_visible_items(14)
        .select_one(0)?;
    if choice == discovered.len() {
        ask_manual_model()
    } else {
        let model = discovered[choice].clone();
        success(format!("Model selected: {model}"));
        Ok(model)
    }
}

fn ask_manual_model() -> Result<String> {
    let model = ask_required("Model id")?;
    success(format!("Model selected: {model}"));
    Ok(model)
}

pub async fn discover_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let resp = client.get(url).bearer_auth(api_key).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        bail!("provider returned {status}: {text}");
    }
    let body: ModelsResponse = resp.json().await?;
    Ok(body
        .data
        .into_iter()
        .map(|item| item.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect())
}

pub fn apply_setup(root: &Path, selection: &SetupSelection) -> Result<()> {
    apply_setups(root, std::slice::from_ref(selection), 0)
}

pub fn apply_setups(root: &Path, selections: &[SetupSelection], active_index: usize) -> Result<()> {
    if selections.is_empty() {
        bail!("at least one provider must be configured");
    }
    let Some(active) = selections.get(active_index) else {
        bail!("active provider selection is out of range");
    };
    for selection in selections {
        validate_provider_id(&selection.provider_id)?;
        validate_base_url(&selection.base_url)?;
        if selection.api_key_env.trim().is_empty() {
            bail!("API key environment variable must not be empty");
        }
        if selection.model_id.trim().is_empty() {
            bail!("model id must not be empty");
        }
    }

    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    let mut config_file = load_config_or_default(root)?;
    let mut providers = load_providers_or_empty(root)?;
    let mut models = load_models_or_empty(root)?;

    for selection in selections {
        upsert_provider(&mut providers, selection);
        upsert_model(&mut models, selection);
    }

    config_file.default_provider = Some(active.provider_id.clone());
    config_file.default_model = Some(active.model_id.clone());

    write_json_pretty(&config::providers_path(root), &providers)?;
    write_json_pretty(&config::models_path(root), &models)?;
    write_json_pretty(&config::config_path(root), &config_file)?;
    Ok(())
}

fn upsert_provider(providers: &mut ProvidersFile, selection: &SetupSelection) {
    let new_entry = ProviderDefinition {
        id: selection.provider_id.clone(),
        name: Some(selection.provider_name.clone()),
        kind: DEFAULT_PROVIDER_KIND.to_string(),
        base_url: selection.base_url.clone(),
        api_key: format!("${}", selection.api_key_env),
        default_model: Some(selection.model_id.clone()),
        models: vec![selection.model_id.clone()],
    };

    if let Some(existing) = providers
        .providers
        .iter_mut()
        .find(|provider| provider.id == selection.provider_id)
    {
        let mut model_ids: BTreeSet<String> = existing.models.iter().cloned().collect();
        model_ids.insert(selection.model_id.clone());
        *existing = ProviderDefinition {
            models: model_ids.into_iter().collect(),
            ..new_entry
        };
    } else {
        providers.providers.push(new_entry);
    }
}

fn upsert_model(models: &mut ModelsFile, selection: &SetupSelection) {
    let model = ModelDefinition {
        id: selection.model_id.clone(),
        provider: selection.provider_id.clone(),
        display_name: None,
        context_length: None,
        max_output_tokens: None,
        supports_tools: selection.supports_tools,
        supports_streaming: true,
        reasoning: ReasoningMetadata {
            supported: selection.supports_reasoning,
            required: false,
            default_effort: if selection.supports_reasoning {
                ReasoningEffort::Medium
            } else {
                ReasoningEffort::Off
            },
            request_format: ReasoningRequestFormat::ReasoningEffort,
        },
    };

    if let Some(existing) = models.models.iter_mut().find(|existing| {
        existing.provider == selection.provider_id && existing.id == selection.model_id
    }) {
        *existing = model;
    } else {
        models.models.push(model);
    }
}

fn load_config_or_default(root: &Path) -> Result<ConfigFile> {
    Ok(config::load_config_file(root)?.unwrap_or_default())
}

fn load_providers_or_empty(root: &Path) -> Result<ProvidersFile> {
    let path = config::providers_path(root);
    if !path.exists() {
        return Ok(ProvidersFile { providers: vec![] });
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn load_models_or_empty(root: &Path) -> Result<ModelsFile> {
    let path = config::models_path(root);
    if !path.exists() {
        return Ok(ModelsFile { models: vec![] });
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn validate_provider_id(id: &str) -> Result<()> {
    if id.trim().is_empty() {
        bail!("provider id must not be empty");
    }
    if !id.chars().all(|ch| {
        ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-' || ch == '.'
    }) {
        bail!("provider id may contain only lowercase letters, numbers, `_`, `-`, and `.`");
    }
    Ok(())
}

fn validate_base_url(base_url: &str) -> Result<()> {
    let url = reqwest::Url::parse(base_url).context("base URL must be an absolute URL")?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        other => bail!("base URL must use http or https, not `{other}`"),
    }
}

fn looks_like_env_var(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn needs_initial_setup(root: &Path) -> bool {
    let config_exists = config::config_path(root).exists();
    let providers_exists = config::providers_path(root).exists();
    let models_exists = config::models_path(root).exists();

    if !config_exists && !providers_exists && !models_exists {
        return true;
    }
    if config_exists {
        return false;
    }
    if !providers_exists || !models_exists {
        return true;
    }

    providers_are_default(root).unwrap_or(false) && models_are_default(root).unwrap_or(false)
}

fn providers_are_default(root: &Path) -> Result<bool> {
    let providers = load_providers_or_empty(root)?;
    let default = config::default_provider_definition();
    Ok(providers.providers.len() == 1
        && providers.providers[0].id == default.id
        && providers.providers[0].kind == default.kind
        && providers.providers[0].base_url == default.base_url
        && providers.providers[0].api_key == default.api_key
        && providers.providers[0].default_model == default.default_model
        && providers.providers[0].models == default.models)
}

fn models_are_default(root: &Path) -> Result<bool> {
    let models = load_models_or_empty(root)?;
    let default = config::default_model_definition();
    Ok(models.models.len() == 1
        && models.models[0].id == default.id
        && models.models[0].provider == default.provider)
}

fn existing_setup_files(root: &Path) -> bool {
    config::providers_path(root).exists()
        || config::models_path(root).exists()
        || config::config_path(root).exists()
}

fn ask_required(prompt: &str) -> Result<String> {
    TextPrompt::new(prompt).required(true).prompt()
}

fn ask_default(prompt: &str, default: &str) -> Result<String> {
    TextPrompt::new(prompt).with_default(default).prompt()
}

fn ask_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    choose_bool(prompt, default_yes)
}

fn choose_bool(prompt: &str, default_yes: bool) -> Result<bool> {
    let initial = if default_yes { 0 } else { 1 };
    let choice =
        Menu::new(prompt, vec![MenuItem::new("Yes"), MenuItem::new("No")]).select_one(initial)?;
    Ok(choice == 0)
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n")).with_context(|| format!("writing {}", path.display()))
}
