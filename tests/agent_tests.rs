use cassady::access::AccessMode;
use cassady::agent::{run_turn, AgentEvent, AgentSettings};
use cassady::config::Config;
use cassady::conversation::{Conversation, Record};
use tempfile::tempdir;
use tokio::sync::mpsc;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
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
