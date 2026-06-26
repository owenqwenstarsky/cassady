use cassady::access::AccessMode;
use cassady::agent::{run_turn, run_turn_with_commands, AgentCommand, AgentEvent, AgentSettings};
use cassady::config::{Config, ReasoningEffort, ReasoningRequestFormat};
use cassady::conversation::{Conversation, Record};
use tempfile::tempdir;
use tokio::sync::mpsc;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
}

fn tool_call_sse(id: &str, name: &str, arguments: &str) -> ResponseTemplate {
    sse(&format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"{id}\",\"type\":\"function\",\"function\":{{\"name\":\"{name}\",\"arguments\":{}}}}}]}}}}]}}\r\n\r\ndata: [DONE]\r\n\r\n",
        serde_json::to_string(arguments).unwrap()
    ))
}

#[tokio::test]
async fn reasoning_effort_is_sent_as_top_level_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"reasoning_effort\":\"high\""))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Done.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel::<AgentEvent>();

    run_turn(
        conversation,
        "use high reasoning".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::High,
        },
        tx,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn reasoning_effort_supports_reasoning_object_format() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"reasoning\":{\"effort\":\"low\"}"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Done.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let mut model_metadata = cassady::config::default_model_definition();
    model_metadata.reasoning.request_format = ReasoningRequestFormat::ReasoningObject;
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        model_metadata: Some(model_metadata),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel::<AgentEvent>();

    run_turn(
        conversation,
        "use object reasoning".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Low,
        },
        tx,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn fast_mode_preference_does_not_change_openai_compatible_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Done.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        default_fast_mode: true,
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel::<AgentEvent>();

    run_turn(
        conversation,
        "stay compatible".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Off,
        },
        tx,
    )
    .await
    .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(!body.contains("\"effort\":\"minimal\""));
    assert!(!body.contains("\"fast_mode\""));
}

#[tokio::test]
async fn reasoning_is_streamed_persisted_and_sent_back() {
    let server = MockServer::start().await;

    let reasoning_token = "internal-cass-reasoning-token";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains(&format!(
            "\"reasoning_content\":\"{reasoning_token}\""
        )))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Second.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"internal-cass-reasoning-token\",\"content\":\"First.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(10)
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };

    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    let updated = run_turn(
        conversation,
        "first".into(),
        AgentSettings {
            config: config.clone(),
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Off,
        },
        tx,
    )
    .await
    .unwrap();

    let mut streamed_reasoning = String::new();
    while let Ok(event) = rx.try_recv() {
        if let AgentEvent::ReasoningChunk(chunk) = event {
            streamed_reasoning.push_str(&chunk);
        }
    }
    assert_eq!(streamed_reasoning, reasoning_token);
    assert!(matches!(
        updated.records.last().unwrap(),
        Record::Assistant {
            content,
            reasoning,
            reasoning_field,
            ..
        } if content == "First."
            && reasoning == reasoning_token
            && reasoning_field.as_deref() == Some("reasoning_content")
    ));

    let (tx, _rx) = mpsc::unbounded_channel::<AgentEvent>();
    run_turn(
        updated,
        "second".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Off,
        },
        tx,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn empty_final_response_is_reprompted_and_persisted() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("previous response contained no user-facing text"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Done.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(10)
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };

    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    let updated = run_turn(
        conversation,
        "finish empty once".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Off,
        },
        tx,
    )
    .await
    .unwrap();

    let mut streamed = String::new();
    let mut saw_retry_status = false;
    let mut saw_finished = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            AgentEvent::AssistantChunk(chunk) => streamed.push_str(&chunk),
            AgentEvent::Status(status) => {
                saw_retry_status |= status.contains("empty final response");
            }
            AgentEvent::TurnFinished => saw_finished = true,
            _ => {}
        }
    }

    assert!(saw_retry_status);
    assert!(saw_finished);
    assert_eq!(streamed, "Done.");
    let last = updated.records.last().unwrap();
    assert!(matches!(
        last,
        Record::Assistant { content, tool_calls, .. }
            if content == "Done." && tool_calls.is_empty()
    ));
}

#[tokio::test]
async fn tool_results_are_stored_full_but_sent_to_model_with_limit_guidance() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("Cass truncated this tool output"))
        .and(body_string_contains("large.txt lines 1-200"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Done.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(tool_call_sse(
            "call_read",
            "read",
            r#"{"files":[{"path":"large.txt"}]}"#,
        ))
        .with_priority(10)
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let large = (1..=200)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(cwd.path().join("large.txt"), large).unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        model_tool_result_limit: 180,
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel::<AgentEvent>();

    let updated = run_turn(
        conversation,
        "read the large file".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::ReadOnly,
            reasoning_effort: ReasoningEffort::Off,
        },
        tx,
    )
    .await
    .unwrap();

    assert!(updated.records.iter().any(|record| matches!(
        record,
        Record::Tool { name, content, .. }
            if name == "read"
                && content.contains("line 200")
                && !content.contains("Cass truncated this tool output")
    )));
}

#[tokio::test]
async fn workspace_edit_shell_does_not_execute_until_approved() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(tool_call_sse(
            "call_shell",
            "shell",
            r#"{"command":"touch marker"}"#,
        ))
        .with_priority(10)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("exit code: 0"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Approved.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (command_tx, command_rx) = mpsc::unbounded_channel::<AgentCommand>();
    let marker = cwd.path().join("marker");

    let handle = tokio::spawn(run_turn_with_commands(
        conversation,
        "run shell".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::WorkspaceEdit,
            reasoning_effort: ReasoningEffort::Off,
        },
        event_tx,
        command_rx,
    ));

    let request_id = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        if let AgentEvent::ApprovalRequested { request_id, .. } = event {
            break request_id;
        }
    };
    assert!(!marker.exists());
    command_tx
        .send(AgentCommand::ApprovalDecision {
            request_id,
            approved: true,
        })
        .unwrap();

    let updated = handle.await.unwrap().unwrap();
    assert!(marker.exists());
    assert!(updated.records.iter().any(|record| matches!(
        record,
        Record::Tool { name, ok, content, .. }
            if name == "shell" && *ok && content.contains("exit code: 0")
    )));
}

#[tokio::test]
async fn workspace_edit_denied_shell_appends_failed_tool_result_without_execution() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(tool_call_sse(
            "call_shell",
            "shell",
            r#"{"command":"touch marker"}"#,
        ))
        .with_priority(10)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("user denied approval"))
        .respond_with(sse(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Denied.\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
        ))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let config = Config {
        root: root.path().to_path_buf(),
        docs_dir: docs.path().to_path_buf(),
        model: "test-model".into(),
        active_provider: cassady::config::ResolvedProviderConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            ..Config::default().active_provider
        },
        ..Config::default()
    };
    let conversation = Conversation::create(
        &config.conversations_dir(),
        &config.model,
        cwd.path(),
        "base prompt".into(),
    )
    .unwrap();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (command_tx, command_rx) = mpsc::unbounded_channel::<AgentCommand>();
    let marker = cwd.path().join("marker");

    let handle = tokio::spawn(run_turn_with_commands(
        conversation,
        "run shell".into(),
        AgentSettings {
            config,
            cwd: cwd.path().to_path_buf(),
            mode: AccessMode::WorkspaceEdit,
            reasoning_effort: ReasoningEffort::Off,
        },
        event_tx,
        command_rx,
    ));

    let request_id = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        if let AgentEvent::ApprovalRequested { request_id, .. } = event {
            break request_id;
        }
    };
    command_tx
        .send(AgentCommand::ApprovalDecision {
            request_id,
            approved: false,
        })
        .unwrap();

    let updated = handle.await.unwrap().unwrap();
    assert!(!marker.exists());
    assert!(updated.records.iter().any(|record| matches!(
        record,
        Record::Tool { name, ok, content, .. }
            if name == "shell" && !*ok && content.contains("user denied approval")
    )));
}
