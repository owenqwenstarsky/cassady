use cassady::config::{self, ConfigFile, ModelsFile, ProvidersFile, ReasoningEffort};
use cassady::setup::{self, SetupSelection};
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn provider_catalog_contains_expected_providers() {
    let catalog = setup::provider_catalog();
    let ids: Vec<_> = catalog.iter().map(|entry| entry.id).collect();

    assert_eq!(
        ids,
        vec![
            "openai",
            "chatgpt-codex",
            "xai",
            "fireworks",
            "groq",
            "openrouter",
            "opencode-zen",
            "opencode-go",
            "cerebras",
            "novita",
            "together",
        ]
    );
    assert!(catalog
        .iter()
        .all(|entry| entry.base_url.starts_with("https://")));
    assert_eq!(
        catalog
            .iter()
            .find(|entry| entry.id == "opencode-zen")
            .unwrap()
            .base_url,
        "https://opencode.ai/zen/v1"
    );
    assert_eq!(
        catalog
            .iter()
            .find(|entry| entry.id == "opencode-go")
            .unwrap()
            .base_url,
        "https://opencode.ai/zen/go/v1"
    );
}

#[test]
fn apply_setup_upserts_selected_provider_model_and_preserves_unrelated_entries() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("providers.json"),
        r#"{
  "providers": [
    {
      "id": "existing",
      "kind": "openai-compatible",
      "base_url": "https://existing.example/v1",
      "api_key": "$EXISTING_API_KEY",
      "default_model": "existing-model",
      "models": ["existing-model"]
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
      "id": "existing-model",
      "provider": "existing"
    }
  ]
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("config.json"),
        r#"{
  "default_access_mode": "workspace-edit",
  "show_reasoning": true
}
"#,
    )
    .unwrap();

    setup::apply_setup(
        root.path(),
        &SetupSelection {
            provider_id: "groq".into(),
            provider_name: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key_env: "GROQ_API_KEY".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            supports_tools: true,
            supports_reasoning: false,
        },
    )
    .unwrap();

    let providers: ProvidersFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("providers.json")).unwrap())
            .unwrap();
    assert!(providers.providers.iter().any(|p| p.id == "existing"));
    let groq = providers
        .providers
        .iter()
        .find(|provider| provider.id == "groq")
        .unwrap();
    assert_eq!(groq.api_key, "$GROQ_API_KEY");
    assert_eq!(
        groq.default_model.as_deref(),
        Some("llama-3.3-70b-versatile")
    );

    let models: ModelsFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("models.json")).unwrap())
            .unwrap();
    assert!(models
        .models
        .iter()
        .any(|model| model.provider == "existing" && model.id == "existing-model"));
    let model = models
        .models
        .iter()
        .find(|model| model.provider == "groq" && model.id == "llama-3.3-70b-versatile")
        .unwrap();
    assert!(model.supports_tools);
    assert!(!model.reasoning.supported);
    assert_eq!(model.reasoning.default_effort, ReasoningEffort::Off);

    let config: ConfigFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config.default_provider.as_deref(), Some("groq"));
    assert_eq!(
        config.default_model.as_deref(),
        Some("llama-3.3-70b-versatile")
    );
    assert!(config.show_reasoning.unwrap());
    assert_eq!(
        config.default_access_mode.unwrap().to_string(),
        "workspace-edit"
    );
}

#[test]
fn apply_setup_writes_chatgpt_codex_without_api_key() {
    let root = tempdir().unwrap();

    setup::apply_setup(
        root.path(),
        &SetupSelection {
            provider_id: config::CHATGPT_CODEX_PROVIDER_ID.into(),
            provider_name: config::CHATGPT_CODEX_PROVIDER_NAME.into(),
            base_url: config::CHATGPT_CODEX_RESPONSES_URL.into(),
            api_key_env: String::new(),
            model_id: config::CHATGPT_CODEX_DEFAULT_MODEL.into(),
            supports_tools: true,
            supports_reasoning: true,
        },
    )
    .unwrap();

    let providers: ProvidersFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("providers.json")).unwrap())
            .unwrap();
    let provider = &providers.providers[0];
    assert_eq!(provider.id, config::CHATGPT_CODEX_PROVIDER_ID);
    assert_eq!(provider.kind, config::CHATGPT_CODEX_PROVIDER_KIND);
    assert!(provider.api_key.is_empty());

    let models: ModelsFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("models.json")).unwrap())
            .unwrap();
    assert!(models.models[0].fast_mode.supported);
}

#[test]
fn apply_setups_writes_multiple_providers_and_active_choice() {
    let root = tempdir().unwrap();
    let selections = vec![
        SetupSelection {
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model_id: "gpt-4.1".into(),
            supports_tools: true,
            supports_reasoning: true,
        },
        SetupSelection {
            provider_id: "groq".into(),
            provider_name: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key_env: "GROQ_API_KEY".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            supports_tools: true,
            supports_reasoning: false,
        },
    ];

    setup::apply_setups(root.path(), &selections, 1).unwrap();

    let providers: ProvidersFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("providers.json")).unwrap())
            .unwrap();
    assert_eq!(providers.providers.len(), 2);
    assert!(providers
        .providers
        .iter()
        .any(|provider| provider.id == "openai"));
    assert!(providers
        .providers
        .iter()
        .any(|provider| provider.id == "groq"));

    let config: ConfigFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config.default_provider.as_deref(), Some("groq"));
    assert_eq!(
        config.default_model.as_deref(),
        Some("llama-3.3-70b-versatile")
    );
}

#[test]
fn remove_providers_removes_models_and_preserves_active_provider() {
    let root = tempdir().unwrap();
    let selections = vec![
        SetupSelection {
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model_id: "gpt-4.1".into(),
            supports_tools: true,
            supports_reasoning: true,
        },
        SetupSelection {
            provider_id: "groq".into(),
            provider_name: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key_env: "GROQ_API_KEY".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            supports_tools: true,
            supports_reasoning: false,
        },
    ];
    setup::apply_setups(root.path(), &selections, 0).unwrap();

    let result = setup::remove_providers(root.path(), &["groq".to_string()]).unwrap();

    assert_eq!(result.removed_provider_ids, vec!["groq"]);
    assert_eq!(result.removed_model_count, 1);
    assert_eq!(result.remaining_provider_count, 1);
    assert_eq!(result.active_provider.as_deref(), Some("openai"));
    assert_eq!(result.active_model.as_deref(), Some("gpt-4.1"));

    let providers: ProvidersFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("providers.json")).unwrap())
            .unwrap();
    assert_eq!(providers.providers.len(), 1);
    assert_eq!(providers.providers[0].id, "openai");

    let models: ModelsFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("models.json")).unwrap())
            .unwrap();
    assert_eq!(models.models.len(), 1);
    assert_eq!(models.models[0].provider, "openai");
}

#[test]
fn remove_active_provider_selects_remaining_provider_and_model() {
    let root = tempdir().unwrap();
    let selections = vec![
        SetupSelection {
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model_id: "gpt-4.1".into(),
            supports_tools: true,
            supports_reasoning: true,
        },
        SetupSelection {
            provider_id: "groq".into(),
            provider_name: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key_env: "GROQ_API_KEY".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            supports_tools: true,
            supports_reasoning: false,
        },
    ];
    setup::apply_setups(root.path(), &selections, 1).unwrap();

    let result = setup::remove_providers(root.path(), &["groq".to_string()]).unwrap();

    assert_eq!(result.active_provider.as_deref(), Some("openai"));
    assert_eq!(result.active_model.as_deref(), Some("gpt-4.1"));
    let config: ConfigFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config.default_provider.as_deref(), Some("openai"));
    assert_eq!(config.default_model.as_deref(), Some("gpt-4.1"));
}

#[test]
fn remove_all_providers_clears_active_defaults() {
    let root = tempdir().unwrap();
    setup::apply_setup(
        root.path(),
        &SetupSelection {
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model_id: "gpt-4.1".into(),
            supports_tools: true,
            supports_reasoning: true,
        },
    )
    .unwrap();

    let result = setup::remove_providers(root.path(), &["openai".to_string()]).unwrap();

    assert_eq!(result.remaining_provider_count, 0);
    assert!(result.active_provider.is_none());
    assert!(result.active_model.is_none());

    let config: ConfigFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert!(config.default_provider.is_none());
    assert!(config.default_model.is_none());

    let models: ModelsFile =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("models.json")).unwrap())
            .unwrap();
    assert!(models.models.is_empty());
}

#[test]
fn remove_unknown_provider_fails() {
    let root = tempdir().unwrap();
    setup::apply_setup(
        root.path(),
        &SetupSelection {
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model_id: "gpt-4.1".into(),
            supports_tools: true,
            supports_reasoning: true,
        },
    )
    .unwrap();

    let err = setup::remove_providers(root.path(), &["missing".to_string()]).unwrap_err();
    assert!(err
        .to_string()
        .contains("provider `missing` is not configured"));
}

#[test]
fn needs_initial_setup_detects_empty_and_default_only_roots() {
    let root = tempdir().unwrap();
    assert!(setup::needs_initial_setup(root.path()));

    std::fs::write(
        root.path().join("providers.json"),
        serde_json::to_string_pretty(&ProvidersFile {
            providers: vec![config::default_provider_definition()],
        })
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("models.json"),
        serde_json::to_string_pretty(&ModelsFile {
            models: vec![config::default_model_definition()],
        })
        .unwrap(),
    )
    .unwrap();
    assert!(setup::needs_initial_setup(root.path()));

    std::fs::write(
        root.path().join("config.json"),
        serde_json::to_string_pretty(&ConfigFile {
            default_provider: Some("fireworks".into()),
            default_model: Some(config::DEFAULT_MODEL.into()),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    assert!(!setup::needs_initial_setup(root.path()));
}

#[tokio::test]
async fn discover_models_parses_openai_compatible_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [
                {"id": "model-b"},
                {"id": "model-a"}
            ]
        })))
        .mount(&server)
        .await;

    let models = setup::discover_models(&format!("{}/v1", server.uri()), "test-key")
        .await
        .unwrap();
    assert_eq!(models, vec!["model-b", "model-a"]);
}
