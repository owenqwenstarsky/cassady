use cassady::access::AccessMode;
use cassady::tools::{self, ToolContext};
use serde_json::json;
use tempfile::tempdir;

fn ctx(root: &std::path::Path, mode: AccessMode) -> ToolContext {
    ToolContext {
        mode,
        cwd: root.to_path_buf(),
        read_roots: vec![root.to_path_buf()],
        blocked_write_roots: Vec::new(),
        model_result_limit: 100_000,
        runtime_tx: None,
    }
}

fn ctx_with_docs(root: &std::path::Path, docs: &std::path::Path, mode: AccessMode) -> ToolContext {
    ToolContext {
        mode,
        cwd: root.to_path_buf(),
        read_roots: vec![root.to_path_buf(), docs.to_path_buf()],
        blocked_write_roots: vec![docs.to_path_buf()],
        model_result_limit: 100_000,
        runtime_tx: None,
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
async fn read_only_can_read_and_search_docs_root() {
    let dir = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(docs.path().join("guide.md"), "# Guide\nneedle in docs\n").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let context = ctx_with_docs(dir.path(), docs.path(), AccessMode::ReadOnly);

    let read = tools::execute(
        "read",
        json!({"files":[{"path": docs.path().join("guide.md").display().to_string()}]}),
        &context,
    )
    .await;
    assert!(read.ok);
    assert!(read.content.contains("needle in docs"));

    let grep = tools::execute(
        "grep",
        json!({"query":"needle", "paths":[docs.path().display().to_string()]}),
        &context,
    )
    .await;
    assert!(grep.ok);
    assert!(grep.content.contains("guide.md:2"));

    let outside_read = tools::execute(
        "read",
        json!({"files":[{"path": outside.path().join("secret.txt").display().to_string()}]}),
        &context,
    )
    .await;
    assert!(!outside_read.ok);
    assert!(outside_read.content.contains("escapes read-only root"));
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

#[tokio::test]
async fn full_access_blocks_write_and_edit_under_docs_root() {
    let dir = tempdir().unwrap();
    let docs = tempdir().unwrap();
    std::fs::write(docs.path().join("guide.md"), "original docs\n").unwrap();
    let context = ctx_with_docs(dir.path(), docs.path(), AccessMode::FullAccess);

    let write = tools::execute(
        "write",
        json!({"path": docs.path().join("new.md").display().to_string(), "content":"nope"}),
        &context,
    )
    .await;
    assert!(!write.ok);
    assert!(write.content.contains("writes are blocked"));
    assert!(!docs.path().join("new.md").exists());

    let edit = tools::execute(
        "edit",
        json!({"path": docs.path().join("guide.md").display().to_string(), "edits":[{"old_text":"original", "new_text":"changed"}]}),
        &context,
    )
    .await;
    assert!(!edit.ok);
    assert!(edit.content.contains("writes are blocked"));
    assert_eq!(
        std::fs::read_to_string(docs.path().join("guide.md")).unwrap(),
        "original docs\n"
    );
}

#[tokio::test]
async fn shell_streams_output_chunks() {
    let dir = tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut context = ctx(dir.path(), AccessMode::FullAccess);
    context.runtime_tx = Some(tx);

    let out = tools::execute(
        "shell",
        json!({"command":"printf hello; printf err >&2"}),
        &context,
    )
    .await;

    assert!(out.ok);
    assert!(out.content.contains("stdout:\nhello"));
    assert!(out.content.contains("stderr:\nerr"));

    let mut streamed = String::new();
    while let Ok(event) = rx.try_recv() {
        let cassady::tools::ToolRuntimeEvent::OutputChunk { stream, content } = event;
        streamed.push_str(&format!("{stream}:{content}"));
    }
    assert!(streamed.contains("stdout:hello"));
    assert!(streamed.contains("stderr:err"));
}

#[cfg(unix)]
#[tokio::test]
async fn full_access_blocks_writes_through_symlinked_docs_dir() {
    let dir = tempdir().unwrap();
    let docs = tempdir().unwrap();
    std::os::unix::fs::symlink(docs.path(), dir.path().join("docs_link")).unwrap();
    let context = ctx_with_docs(dir.path(), docs.path(), AccessMode::FullAccess);

    let write = tools::execute(
        "write",
        json!({"path":"docs_link/new.md", "content":"nope"}),
        &context,
    )
    .await;
    assert!(!write.ok);
    assert!(write.content.contains("writes are blocked"));
    assert!(!docs.path().join("new.md").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn full_access_blocks_lexical_writes_under_docs_even_when_symlink_points_outside() {
    let dir = tempdir().unwrap();
    let docs = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), docs.path().join("outside_link")).unwrap();
    let context = ctx_with_docs(dir.path(), docs.path(), AccessMode::FullAccess);

    let write = tools::execute(
        "write",
        json!({"path": docs.path().join("outside_link/new.md").display().to_string(), "content":"nope"}),
        &context,
    )
    .await;
    assert!(!write.ok);
    assert!(write.content.contains("writes are blocked"));
    assert!(!outside.path().join("new.md").exists());
}
