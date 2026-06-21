use cassady::access::AccessMode;
use cassady::tools::{self, ToolContext};
use serde_json::json;
use tempfile::tempdir;

fn ctx(root: &std::path::Path, mode: AccessMode) -> ToolContext {
    ToolContext {
        mode,
        cwd: root.to_path_buf(),
        read_only_root: root.to_path_buf(),
        model_result_limit: 100_000,
    }
}

#[tokio::test]
async fn read_only_rejects_write_and_path_escape() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("x.txt"), "secret").unwrap();

    let out = tools::execute(
        "write",
        json!({"path":"x.txt", "content":"hi"}),
        &ctx(dir.path(), AccessMode::ReadOnly),
    )
    .await;
    assert!(!out.ok);

    let out = tools::execute(
        "read",
        json!({"files":[{"path": outside.path().join("x.txt").display().to_string()}]}),
        &ctx(dir.path(), AccessMode::ReadOnly),
    )
    .await;
    assert!(!out.ok);
    assert!(out.content.contains("escapes read-only root"));
}

#[tokio::test]
async fn read_and_grep_work() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo needle\nthree\n").unwrap();
    let context = ctx(dir.path(), AccessMode::ReadOnly);

    let read = tools::execute(
        "read",
        json!({"files":[{"path":"a.txt", "lines":"2-3"}]}),
        &context,
    )
    .await;
    assert!(read.ok);
    assert!(read.content.contains("two needle"));
    assert!(!read.content.contains("one"));

    let grep = tools::execute(
        "grep",
        json!({"query":"needle", "paths":["."], "case_sensitive": true}),
        &context,
    )
    .await;
    assert!(grep.ok);
    assert!(grep.content.contains("a.txt:2"));
}

#[tokio::test]
async fn edit_is_exact_and_atomic_on_validation_failure() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha\nbeta\ngamma\n").unwrap();
    let context = ctx(dir.path(), AccessMode::FullAccess);

    let ok = tools::execute(
        "edit",
        json!({"path":"a.txt", "edits":[{"old_text":"beta", "new_text":"BETA"}]}),
        &context,
    )
    .await;
    assert!(ok.ok);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\nBETA\ngamma\n"
    );

    let bad = tools::execute(
        "edit",
        json!({"path":"a.txt", "edits":[{"old_text":"missing", "new_text":"NOPE"}]}),
        &context,
    )
    .await;
    assert!(!bad.ok);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\nBETA\ngamma\n"
    );
}
