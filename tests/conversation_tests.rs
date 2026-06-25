use cassady::conversation::{list_chats, Conversation, Record};
use tempfile::tempdir;

#[test]
fn conversation_appends_loads_and_lists_by_cwd() {
    let root = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let mut convo =
        Conversation::create(root.path(), "model", cwd.path(), "base prompt".into()).unwrap();
    convo
        .append(Record::User {
            content: "hello world".into(),
            ts: "now".into(),
        })
        .unwrap();

    let (loaded, warning) = Conversation::load(root.path(), &convo.id).unwrap();
    assert!(warning.is_none());
    assert_eq!(loaded.base_system_prompt(), "base prompt");

    let chats = list_chats(root.path(), cwd.path()).unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].id, convo.id);
    assert_eq!(chats[0].first_user_preview, "hello world");
}

#[test]
fn legacy_meta_without_branch_fields_still_loads() {
    let root = tempdir().unwrap();
    let id = "legacy";
    std::fs::write(
        root.path().join(format!("{id}.jsonl")),
        r#"{"type":"meta","chat_id":"legacy","created_at":"now","model":"m","cwd":"/tmp"}
{"type":"system","content":"base"}
"#,
    )
    .unwrap();

    let (loaded, warning) = Conversation::load(root.path(), id).unwrap();
    assert!(warning.is_none());
    let meta = loaded.meta().unwrap();
    assert_eq!(meta.parent_chat_id, None);
    assert_eq!(meta.branch_from, None);
}
