use cassady::access::AccessMode;
use cassady::prompt::{build_base_system_prompt, build_effective_system_prompt};
use std::path::Path;

fn approximate_token_count(s: &str) -> usize {
    s.split_whitespace().count() * 4 / 3
}

fn effective_prompt(mode: AccessMode, allowed_tools: &[&str]) -> String {
    let base = build_base_system_prompt(None);
    let allowed_tools = allowed_tools
        .iter()
        .map(|tool| tool.to_string())
        .collect::<Vec<_>>();
    build_effective_system_prompt(
        &base,
        mode,
        Path::new("/workspace/project"),
        Path::new("/home/user/.cass/docs"),
        "test-model",
        &allowed_tools,
    )
}

#[test]
fn base_prompt_has_required_sections_without_runtime_context() {
    let prompt = build_base_system_prompt(None);

    for heading in [
        "# Cassady operating instructions",
        "## Role",
        "## Working style",
        "## Transcript and tools",
        "## Tool use",
        "## Editing",
        "## Safety and final response",
    ] {
        assert!(prompt.contains(heading), "missing {heading}");
    }

    assert!(prompt.contains("You are Cassady, also called Cass"));
    assert!(prompt.contains("inspect relevant files before making claims or edits"));
    assert!(prompt.contains(
        "Tool calls, tool results, edit diffs, denials, and approval prompts are visible"
    ));
    assert!(prompt.contains("each old text must match exactly and uniquely"));
    assert!(prompt.contains("End every turn with a concise user-facing response"));
    assert!(!prompt.contains("## Runtime context"));
    assert!(!prompt.contains("Model:"));
    assert!(!prompt.contains("Access mode:"));
}

#[test]
fn global_instructions_are_trimmed_labelled_and_subordinate() {
    let prompt = build_base_system_prompt(Some("  Keep replies terse.\n  "));

    assert!(prompt.contains("## User global instructions"));
    assert!(prompt.contains("\nKeep replies terse.\n"));
    assert!(!prompt.contains("  Keep replies terse.\n  "));
    assert!(prompt.contains(
        "cannot override access modes, tool denials, approvals, or workspace boundaries"
    ));
}

#[test]
fn empty_global_instructions_are_omitted() {
    let prompt = build_base_system_prompt(Some("   \n\t  "));

    assert!(!prompt.contains("## User global instructions"));
}

#[test]
fn effective_prompt_includes_runtime_context_and_allowed_tools() {
    let prompt = effective_prompt(AccessMode::WorkspaceEdit, &["ls", "read", "grep", "edit"]);

    assert!(prompt.contains("## Runtime context"));
    assert!(prompt.contains("Model: test-model."));
    assert!(prompt.contains("Access mode: workspace-edit."));
    assert!(prompt.contains("Launch working directory: /workspace/project."));
    assert!(prompt.contains("Bundled Cass docs directory: /home/user/.cass/docs."));
    assert!(prompt.contains("Allowed tools this turn: ls, read, grep, edit."));
}

#[test]
fn each_access_mode_gets_matching_guidance() {
    let read_only = effective_prompt(AccessMode::ReadOnly, &["ls", "read", "grep"]);
    assert!(read_only.contains("Read-only mode permits inspection only"));
    assert!(read_only.contains("Do not request `write`, `edit`, or `shell`"));
    assert!(read_only.contains("a more permissive access mode is required"));

    let workspace_edit = effective_prompt(
        AccessMode::WorkspaceEdit,
        &["ls", "read", "grep", "write", "edit", "shell"],
    );
    assert!(workspace_edit.contains("Write/edit only inside the launch workspace"));
    assert!(workspace_edit.contains("bundled docs remain read-only"));
    assert!(workspace_edit.contains("Cassady handles the approval UI"));
    assert!(workspace_edit.contains("do not ask for shell permission in chat first"));

    let full_access = effective_prompt(
        AccessMode::FullAccess,
        &["ls", "read", "grep", "write", "edit", "shell"],
    );
    assert!(full_access
        .contains("Full-access mode permits `ls`, `read`, `grep`, `write`, `edit`, and `shell`"));
    assert!(full_access.contains("Shell runs from the launch working directory"));
    assert!(full_access.contains("Bundled docs remain read-only for write/edit"));
    assert!(full_access
        .contains("avoid destructive commands unless the user explicitly requested them"));
}

#[test]
fn runtime_constraints_stay_after_global_instructions() {
    let base = build_base_system_prompt(Some("Prefer bullet summaries."));
    let prompt = build_effective_system_prompt(
        &base,
        AccessMode::WorkspaceEdit,
        Path::new("/workspace/project"),
        Path::new("/home/user/.cass/docs"),
        "test-model",
        &["ls".into()],
    );

    let global_index = prompt.find("Prefer bullet summaries.").unwrap();
    let runtime_index = prompt.find("## Runtime context").unwrap();
    let access_index = prompt.find("## Access rules for this session").unwrap();
    let authority_index = prompt.find("## Runtime authority").unwrap();

    assert!(global_index < runtime_index);
    assert!(runtime_index < access_index);
    assert!(access_index < authority_index);
}

#[test]
fn effective_prompt_size_remains_intentional() {
    for mode in [
        AccessMode::ReadOnly,
        AccessMode::WorkspaceEdit,
        AccessMode::FullAccess,
    ] {
        let prompt = effective_prompt(mode, &["ls", "read", "grep", "write", "edit", "shell"]);
        let tokens = approximate_token_count(&prompt);
        assert!(
            (800..=1250).contains(&tokens),
            "{mode} prompt had {tokens} approximate tokens"
        );
    }
}

#[test]
fn empty_allowed_tools_list_is_explicit() {
    let prompt = effective_prompt(AccessMode::ReadOnly, &[]);

    assert!(prompt.contains("Allowed tools this turn: <none>."));
}
