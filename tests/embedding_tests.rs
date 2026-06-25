use cassady::access::AccessMode;
use cassady::config::ReasoningEffort;
use cassady::conversation::Record;
use cassady::embedding::{Event, SessionBuilder};
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
}

fn content_sse(content: &str) -> ResponseTemplate {
    sse(&format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":{}}}}}]}}\r\n\r\ndata: [DONE]\r\n\r\n",
        serde_json::to_string(content).unwrap()
    ))
}

fn tool_call_sse(id: &str, name: &str, arguments: &str) -> ResponseTemplate {
    sse(&format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"{id}\",\"type\":\"function\",\"function\":{{\"name\":\"{name}\",\"arguments\":{}}}}}]}}}}]}}\r\n\r\ndata: [DONE]\r\n\r\n",
        serde_json::to_string(arguments).unwrap()
    ))
}

fn write_test_config(root: &std::path::Path, base_url: &str) {
    std::fs::write(
        root.join("providers.json"),
        serde_json::to_string_pretty(&json!({
            "providers": [{
                "id": "test-provider",
                "kind": "openai-compatible",
                "base_url": base_url,
                "api_key": "test-key",
                "default_model": "test-model",
                "models": ["test-model"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("models.json"),
        serde_json::to_string_pretty(&json!({
            "models": [{
                "id": "test-model",
                "provider": "test-provider",
                "context_length": 128,
                "max_output_tokens": 64,
                "reasoning": {
                    "supported": true,
                    "required": false,
                    "default_effort": "off",
                    "request_format": "reasoning_effort"
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("config.json"),
        serde_json::to_string_pretty(&json!({
            "default_provider": "test-provider",
            "default_model": "test-model",
            "default_reasoning_effort": "off"
        }))
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn embedded_session_runs_turn_and_streams_events() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(content_sse("Hello from embedded Cassady."))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    write_test_config(root.path(), &server.uri());

    let session = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .access_mode(AccessMode::ReadOnly)
        .reasoning_effort(ReasoningEffort::Off)
        .build()
        .await
        .unwrap();

    assert_eq!(session.model(), "test-model");
    assert_eq!(session.access_mode(), AccessMode::ReadOnly);

    let mut turn = session.start_turn("say hi").await.unwrap();
    let mut streamed = String::new();
    while let Some(event) = turn.next_event().await.unwrap() {
        match event {
            Event::AssistantChunk(chunk) => streamed.push_str(&chunk),
            Event::Finished => break,
            _ => {}
        }
    }
    let session = turn.finish().await.unwrap();

    assert_eq!(streamed, "Hello from embedded Cassady.");
    assert!(session.records().iter().any(|record| matches!(
        record,
        Record::Assistant { content, .. } if content == "Hello from embedded Cassady."
    )));
    assert!(session.conversation_path().is_file());
}

#[tokio::test]
async fn builder_overrides_config_for_model_endpoint_key_mode_and_reasoning() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"model\":\"test-model\""))
        .and(body_string_contains("\"reasoning_effort\":\"low\""))
        .respond_with(content_sse("Overrides worked."))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    write_test_config(root.path(), "https://wrong.example/v1");
    let env_name = "CASSADY_EMBEDDING_TEST_KEY";
    let old = std::env::var(env_name).ok();
    std::env::set_var(env_name, "test-key-from-env");

    let session = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .access_mode(AccessMode::WorkspaceEdit)
        .model("test-model")
        .base_url(server.uri())
        .api_key_env(env_name)
        .reasoning_effort(ReasoningEffort::Low)
        .build()
        .await
        .unwrap();

    assert_eq!(session.access_mode(), AccessMode::WorkspaceEdit);
    assert_eq!(session.reasoning_effort(), ReasoningEffort::Low);

    let mut turn = session.start_turn("check overrides").await.unwrap();
    while let Some(event) = turn.next_event().await.unwrap() {
        if matches!(event, Event::Finished) {
            break;
        }
    }
    let _session = turn.finish().await.unwrap();

    if let Some(old) = old {
        std::env::set_var(env_name, old);
    } else {
        std::env::remove_var(env_name);
    }
}

#[tokio::test]
async fn embedded_session_can_resume_existing_conversation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(content_sse("First turn."))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    write_test_config(root.path(), &server.uri());

    let session = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .access_mode(AccessMode::ReadOnly)
        .build()
        .await
        .unwrap();
    let mut turn = session.start_turn("first").await.unwrap();
    while let Some(event) = turn.next_event().await.unwrap() {
        if matches!(event, Event::Finished) {
            break;
        }
    }
    let session = turn.finish().await.unwrap();
    let id = session.id().to_string();
    let record_count = session.records().len();

    let resumed = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .resume(&id)
        .await
        .unwrap();

    assert_eq!(resumed.id(), id);
    assert_eq!(resumed.records().len(), record_count);
    assert!(resumed.resume_warning().is_none());
}

#[tokio::test]
async fn embedded_approval_flow_can_approve_shell() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("exit code: 0"))
        .respond_with(content_sse("Approved shell."))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
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

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    write_test_config(root.path(), &server.uri());
    let marker = cwd.path().join("marker");

    let session = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .access_mode(AccessMode::WorkspaceEdit)
        .build()
        .await
        .unwrap();

    let mut turn = session.start_turn("run shell").await.unwrap();
    let mut saw_request = false;
    let mut saw_resolved = false;
    while let Some(event) = turn.next_event().await.unwrap() {
        match event {
            Event::ApprovalRequested(request) => {
                saw_request = true;
                assert_eq!(request.name, "shell");
                assert!(!marker.exists());
                turn.approve(&request.request_id).unwrap();
            }
            Event::ApprovalResolved { approved, .. } => {
                saw_resolved = approved;
            }
            Event::Finished => break,
            _ => {}
        }
    }
    let session = turn.finish().await.unwrap();

    assert!(saw_request);
    assert!(saw_resolved);
    assert!(marker.exists());
    assert!(session.records().iter().any(|record| matches!(
        record,
        Record::Tool { name, ok, content, .. }
            if name == "shell" && *ok && content.contains("exit code: 0")
    )));
}

#[tokio::test]
async fn read_only_embedding_does_not_advertise_mutating_tools() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(content_sse("Readonly."))
        .expect(1)
        .mount(&server)
        .await;

    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    write_test_config(root.path(), &server.uri());

    let session = SessionBuilder::new()
        .config_root(root.path())
        .cwd(cwd.path())
        .access_mode(AccessMode::ReadOnly)
        .build()
        .await
        .unwrap();
    let mut turn = session.start_turn("inspect only").await.unwrap();
    while let Some(event) = turn.next_event().await.unwrap() {
        if matches!(event, Event::Finished) {
            break;
        }
    }
    let _session = turn.finish().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("\"name\":\"ls\""));
    assert!(body.contains("\"name\":\"read\""));
    assert!(body.contains("\"name\":\"grep\""));
    assert!(!body.contains("\"name\":\"write\""));
    assert!(!body.contains("\"name\":\"edit\""));
    assert!(!body.contains("\"name\":\"shell\""));
}
