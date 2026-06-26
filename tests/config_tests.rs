use cassady::check;
use cassady::cli::Cli;
use cassady::config::{
    self, Config, ModelsFile, ProviderDefinition, ProvidersFile, ReasoningEffort,
    ReasoningRequestFormat,
};
use tempfile::tempdir;

fn cli() -> Cli {
    Cli {
        command: None,
        resume: None,
        model: None,
        base_url: None,
        api_key_env: None,
        cwd: None,
        readonly: false,
        workspace_edit: false,
        full_access: false,
    }
}

#[test]
fn default_provider_and_model_files_are_created() {
    let root = tempdir().unwrap();

    let cfg = Config::load_from_root_with_docs(
        root.path().to_path_buf(),
        root.path().join("docs"),
        &cli(),
    )
    .unwrap();

    assert_eq!(cfg.provider_id, config::DEFAULT_PROVIDER_ID);
    assert_eq!(cfg.model, config::DEFAULT_MODEL);
    assert!(root.path().join("providers.json").is_file());
    assert!(root.path().join("models.json").is_file());

    let providers: ProvidersFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("providers.json")).unwrap())
            .unwrap();
    assert_eq!(providers.providers[0].id, "fireworks");
    assert_eq!(providers.providers[0].api_key, "$FIREWORKS_API_KEY");

    let models: ModelsFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("models.json")).unwrap())
            .unwrap();
    assert_eq!(models.models[0].provider, "fireworks");
    assert!(models.models[0].reasoning.supported);
    assert!(!models.models[0].fast_mode.supported);
    assert_eq!(
        models.models[0].reasoning.default_effort,
        ReasoningEffort::Medium
    );
    assert_eq!(
        models.models[0].reasoning.request_format,
        ReasoningRequestFormat::ReasoningEffort
    );
}

#[test]
fn legacy_config_connection_fields_still_work() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("config.json"),
        r#"{
  "provider": "openai-compatible",
  "model": "legacy-model",
  "base_url": "https://example.com/v1",
  "api_key_env": "LEGACY_API_KEY"
}
"#,
    )
    .unwrap();

    let cfg = Config::load_from_root_with_docs(
        root.path().to_path_buf(),
        root.path().join("docs"),
        &cli(),
    )
    .unwrap();

    assert_eq!(cfg.provider_id, "openai-compatible");
    assert_eq!(cfg.model, "legacy-model");
    assert_eq!(cfg.active_provider.base_url, "https://example.com/v1");
    assert_eq!(cfg.active_provider.api_key, "$LEGACY_API_KEY");
}

#[test]
fn api_key_resolution_supports_env_refs_and_literals() {
    let key = "CASS_TEST_PROVIDER_KEY";
    let old = std::env::var(key).ok();
    std::env::set_var(key, "secret-value");
    assert_eq!(
        config::resolve_api_key(&format!("${key}")).unwrap(),
        "secret-value"
    );
    assert_eq!(
        config::resolve_api_key("literal-key").unwrap(),
        "literal-key"
    );

    std::env::remove_var(key);
    assert!(config::resolve_api_key(&format!("${key}")).is_err());

    if let Some(old) = old {
        std::env::set_var(key, old);
    }
}

#[test]
fn reasoning_defaults_to_supported_medium_for_model_metadata() {
    let model: config::ModelDefinition = serde_json::from_str(
        r#"{
  "id": "test-model",
  "provider": "test-provider"
}
"#,
    )
    .unwrap();

    assert!(model.reasoning.supported);
    assert!(!model.reasoning.required);
    assert_eq!(model.reasoning.default_effort, ReasoningEffort::Medium);
    assert_eq!(
        model.reasoning.request_format,
        ReasoningRequestFormat::ReasoningEffort
    );
}

#[test]
fn fast_mode_defaults_to_off_and_unsupported() {
    let model: config::ModelDefinition = serde_json::from_str(
        r#"{
  "id": "test-model",
  "provider": "test-provider"
}
"#,
    )
    .unwrap();

    assert!(!model.fast_mode.supported);

    let root = tempdir().unwrap();
    let cfg = Config::load_from_root_with_docs(
        root.path().to_path_buf(),
        root.path().join("docs"),
        &cli(),
    )
    .unwrap();
    let state = cfg.fast_mode_state();
    assert!(!state.preferred);
    assert!(!state.supported);
    assert!(!state.active);
}

#[test]
fn fast_mode_preference_persists_without_losing_config_fields() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("config.json"),
        r#"{
  "default_access_mode": "workspace-edit",
  "show_reasoning": true
}
"#,
    )
    .unwrap();

    config::save_fast_mode_preference(root.path(), true).unwrap();

    let cfg = Config::load_from_root_with_docs(
        root.path().to_path_buf(),
        root.path().join("docs"),
        &cli(),
    )
    .unwrap();
    assert!(cfg.default_fast_mode);
    assert!(cfg.show_reasoning);
    assert_eq!(cfg.default_access_mode.to_string(), "workspace-edit");
}

#[test]
fn fast_mode_state_is_active_for_supported_codex_model() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("providers.json"),
        r#"{
  "providers": [
    {
      "id": "chatgpt-codex",
      "name": "ChatGPT Codex",
      "kind": "chatgpt-codex",
      "base_url": "https://chatgpt.com/backend-api/codex/responses",
      "api_key": "",
      "default_model": "gpt-5.5",
      "models": ["gpt-5.5"]
    }
  ]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("models.json"),
        r#"{
  "models": [
    {
      "id": "gpt-5.5",
      "provider": "chatgpt-codex",
      "fast_mode": { "supported": true }
    }
  ]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("config.json"),
        r#"{
  "default_provider": "chatgpt-codex",
  "default_model": "gpt-5.5",
  "default_fast_mode": true
}
"#,
    )
    .unwrap();

    let cfg = Config::load_from_root_with_docs(
        root.path().to_path_buf(),
        root.path().join("docs"),
        &cli(),
    )
    .unwrap();
    let state = cfg.fast_mode_state();
    assert!(state.preferred);
    assert!(state.supported);
    assert!(state.active);
}

#[test]
fn validation_accepts_chatgpt_codex_without_api_key() {
    let providers = ProvidersFile {
        providers: vec![ProviderDefinition {
            id: config::CHATGPT_CODEX_PROVIDER_ID.into(),
            name: Some(config::CHATGPT_CODEX_PROVIDER_NAME.into()),
            kind: config::CHATGPT_CODEX_PROVIDER_KIND.into(),
            base_url: config::CHATGPT_CODEX_RESPONSES_URL.into(),
            api_key: String::new(),
            default_model: Some(config::CHATGPT_CODEX_DEFAULT_MODEL.into()),
            models: vec![config::CHATGPT_CODEX_DEFAULT_MODEL.into()],
        }],
    };
    let models = ModelsFile {
        models: vec![config::ModelDefinition {
            id: config::CHATGPT_CODEX_DEFAULT_MODEL.into(),
            provider: config::CHATGPT_CODEX_PROVIDER_ID.into(),
            display_name: None,
            context_length: None,
            max_output_tokens: None,
            supports_tools: true,
            supports_streaming: true,
            reasoning: Default::default(),
            fast_mode: Default::default(),
        }],
    };

    let summary = config::validate_registries(None, &providers, &models);

    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
}

#[test]
fn validation_rejects_duplicate_provider_ids() {
    let providers = ProvidersFile {
        providers: vec![
            config::default_provider_definition(),
            ProviderDefinition {
                id: "fireworks".into(),
                name: None,
                kind: "openai-compatible".into(),
                base_url: "https://example.com/v1".into(),
                api_key: "$OTHER_KEY".into(),
                default_model: None,
                models: Vec::new(),
            },
        ],
    };
    let models = ModelsFile {
        models: vec![config::default_model_definition()],
    };

    let summary = config::validate_registries(None, &providers, &models);
    assert!(summary
        .errors
        .iter()
        .any(|error| error.contains("duplicate provider id `fireworks`")));
}

#[test]
fn check_reports_invalid_json() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("providers.json"), "{ invalid json").unwrap();

    let report = check::run_with_root(root.path().to_path_buf(), &cli()).unwrap();

    assert!(report.has_errors());
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("providers.json")));
}

#[test]
fn check_passes_for_valid_literal_key_without_leaking_it() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("providers.json"),
        r#"{
  "providers": [
    {
      "id": "test-provider",
      "kind": "openai-compatible",
      "base_url": "https://example.com/v1",
      "api_key": "super-secret-literal",
      "default_model": "test-model",
      "models": ["test-model"]
    }
  ]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("models.json"),
        r#"{
  "models": [
    {
      "id": "test-model",
      "provider": "test-provider",
      "context_length": 128,
      "max_output_tokens": 64
    }
  ]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("config.json"),
        r#"{
  "default_provider": "test-provider",
  "default_model": "test-model"
}
"#,
    )
    .unwrap();

    let report = check::run_with_root(root.path().to_path_buf(), &cli()).unwrap();
    let rendered = report.render();

    assert!(!report.has_errors(), "{rendered}");
    assert!(!rendered.contains("super-secret-literal"));
    assert!(rendered.contains("api key: literal value configured"));
}
