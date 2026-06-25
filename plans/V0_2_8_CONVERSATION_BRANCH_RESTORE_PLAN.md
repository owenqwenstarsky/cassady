# v0.2.8 Conversation Branch and Restore Implementation Plan

## Goal

v0.2.8 adds an in-chat branch and restore menu opened by pressing `Esc` twice while Cassady is idle. Users should be able to browse the current conversation timeline, choose a checkpoint at a user message, assistant message, or tool call, and branch from that point without destroying the original conversation. They can optionally restore Cassady-tracked file edits to match the selected checkpoint, or branch the conversation only.

Success statement:

> A user can press `Esc` twice, select an earlier message or tool call, create a new branch from that point, optionally restore tracked file edits, and later open the same menu from either branch to switch or branch again from the related conversation history.

## Scope

### In scope

- Add a double-`Esc` idle shortcut that opens a branch/restore menu, similar in feel to the double-`Ctrl-C` exit affordance.
- Keep the existing busy `Esc` behavior for turn cancellation and approval denial.
- Add a branch-aware conversation model that creates a new conversation file when restoring to a checkpoint instead of truncating or overwriting the original chat.
- Let users browse checkpoints for:
  - user messages,
  - assistant messages,
  - assistant tool-call requests,
  - completed tool results.
- Preserve valid model conversation structure when branching at or around tool calls.
- Track file mutations made by Cassady's `write` and `edit` tools with enough before/after data to restore workspace files backward or forward between tracked checkpoints.
- Offer restore actions that clearly separate conversation-only branching from conversation-plus-file restoration.
- Allow users to return to the original conversation or other related branches by opening the menu again.
- Add tests for branch metadata, checkpoint extraction, valid tool-call repair, file-edit journaling, workspace restore planning, conflict detection, and keybinding behavior where practical.
- Update README and bundled docs for the new shortcut, branch semantics, file-restore limitations, and safety prompts.

### Out of scope

- Rewriting arbitrary filesystem changes made by shell commands, editors, package managers, test runners, or the user outside Cassady's `write`/`edit` tools.
- Git integration, commits, worktrees, or automatic VCS operations.
- A visual diff editor for every file restore. v0.2.8 should show a concise restore plan and rely on safe conflict checks.
- Merging branches or replaying assistant responses across branches.
- Branching while an agent turn is running.
- Changing provider message semantics beyond the minimum repair needed for valid branched conversations.

## Context and Current State

Relevant files and behavior:

- `src/app.rs` owns the TUI event loop, current conversation, transcript blocks, double-`Ctrl-C` exit behavior, busy `Esc` cancellation, local `/new` and `/resume` commands, and turn spawning.
- `src/conversation.rs` stores conversations as append-only JSONL with `Meta`, `System`, `User`, `Assistant`, and `Tool` records. There is no branch metadata, checkpoint API, or rewrite/create-from-prefix helper yet.
- `src/agent.rs` appends user, assistant, and tool records during a turn. Assistant records can contain multiple tool calls, while each tool result is a separate `Record::Tool`.
- `src/tools/edit.rs` and `src/tools/write.rs` perform atomic writes and return user-visible summaries/diffs, but they do not persist before/after snapshots that can be used for later restore.
- `src/ui/render.rs` renders the main chat. A branch menu should be integrated as an in-TUI modal or state, not by dropping into the setup/update prompt menu in `src/menu.rs`.

The key design constraint is that restore must not mean destructive truncation. Selecting an old point creates a new branch conversation and leaves the source conversation available.

## Design Principles

1. **Branch, do not erase.** Restoring conversation state always creates or switches to a conversation branch; the original JSONL file remains intact.
2. **Make file restore explicit.** Conversation branching is safe and default. File restoration is a separate confirmation because it changes the workspace.
3. **Keep model history valid.** Branches created at tool boundaries must not leave assistant tool calls without corresponding tool records.
4. **Track only what Cassady can prove.** File restore uses durable snapshots from `write`/`edit`; unsupported shell/user changes are detected or warned about, not guessed.
5. **Recoverable navigation.** Every branch keeps parent/checkpoint metadata so the menu can show the related branch family and let users switch or branch again.
6. **Small, testable modules.** Put checkpoint extraction, branch creation, and file restore planning in dedicated modules rather than expanding the TUI loop with business logic.

## User Experience

### Shortcut behavior

- While idle, first `Esc` sets status text:

  ```text
  press Esc again within 1.5s to branch or restore
  ```

- A second `Esc` within the same window opens the branch/restore menu.
- If the input box is non-empty, do not discard it silently. The first `Esc` should keep the input and show the same status; opening the menu should preserve the draft input if the user cancels.
- While an agent turn is running, keep the current behavior: `Esc` cancels the active turn. Do not open the branch menu while busy.
- During approval prompts, keep `Esc` as denial for the approval request.

### Main branch menu

The menu should show the current branch family, not only the current JSONL prefix:

```text
Branch / Restore

Current chat: 2026-06-25-101533-abcd

Branches
  • current branch
  • original chat from before restore
  • earlier branch: "try without refactor"

Timeline
  1. user      Add tests for config loading
  2. assistant Proposed plan
  3. tool      read src/config.rs ✓
  4. assistant Found config parser
  5. tool      edit src/config.rs ✓  file: src/config.rs
  6. user      Make it cleaner
```

Keyboard controls should be consistent with the main TUI: up/down or `j`/`k` move, Enter selects, `Esc` cancels, and an optional `/` filter can be deferred unless cheap.

### Checkpoint actions

After selecting a checkpoint, show an action menu:

```text
Branch from checkpoint

Selected: tool edit src/config.rs at 10:24:11

  1. Branch conversation only
  2. Branch conversation and restore tracked file edits
  3. Preview tracked file restore plan
  4. Cancel
```

Default should be conversation-only. The branch should get a fresh chat id, copy records through the selected checkpoint, and append branch metadata. The status should make the branch explicit:

```text
branched 2026-06-25-110212-wxyz from 2026-06-25-101533-abcd at tool edit src/config.rs
```

### Switching among related branches

Opening the menu from a branch should show its ancestors and descendants. Users can switch back to an existing branch without creating another branch:

```text
Switch to branch

  original  2026-06-25-101533-abcd  18 records
  current   2026-06-25-110212-wxyz  branched at tool edit src/config.rs
```

Switching branch changes the active conversation/transcript only. It should not change files unless the user explicitly chooses a file restore action.

### File restore safety

When the user chooses file restoration, show a concise plan before writing:

```text
Restore tracked file edits to selected checkpoint?

Will update:
  src/config.rs        current hash matches Cassady snapshot
  src/app.rs           current hash differs; requires confirmation or skip

Will delete:
  src/generated.rs     created after the checkpoint by Cassady write

Not tracked:
  shell command outputs and manual edits cannot be restored automatically

Proceed? [y/N]
```

Rules:

- If the current file hash matches the expected tracked hash, restore automatically after confirmation.
- If the file changed outside Cassady since the relevant snapshot, mark it as a conflict and default to skipping or cancelling the whole restore.
- For files that did not exist at the target checkpoint, delete only if the current content hash matches the tracked created-file hash.
- Never overwrite unknown current content without an explicit conflict confirmation.

## Design

### Conversation branch metadata

Extend the conversation metadata in a backward-compatible way. One acceptable shape is adding optional fields to `Record::Meta` with `#[serde(default)]` and `skip_serializing_if`:

```rust
Record::Meta {
    chat_id: String,
    created_at: String,
    model: String,
    cwd: String,
    parent_chat_id: Option<String>,
    branch_from: Option<BranchPoint>,
}

struct BranchPoint {
    chat_id: String,
    record_index: usize,
    tool_call_id: Option<String>,
    checkpoint_label: String,
}
```

Older conversations load with no parent. Descendants can be discovered by scanning `config.conversations_dir()` for `Meta.parent_chat_id` references.

Add a `conversation::create_branch(...)` helper that:

1. loads the source conversation,
2. computes a valid record prefix for the selected checkpoint,
3. writes a new JSONL file with a fresh chat id and branch metadata,
4. preserves the original `System` prompt and source metadata needed for branch navigation,
5. returns the new `Conversation` for the TUI to load immediately.

Do not truncate or rewrite the source conversation.

### Checkpoint extraction

Add a focused module such as `src/branch.rs` or `src/conversation_branch.rs` with types like:

```rust
struct Checkpoint {
    id: String,
    chat_id: String,
    record_index: usize,
    tool_call_id: Option<String>,
    kind: CheckpointKind,
    label: String,
    detail: String,
    ts: Option<String>,
}

enum CheckpointKind {
    User,
    Assistant,
    ToolCall,
    ToolResult,
}
```

Checkpoint rules:

- A user checkpoint means the branch includes that user record.
- An assistant checkpoint means the branch includes that assistant record. If the assistant requested tools, the branch helper must repair or omit incomplete tool-call state before the next provider turn.
- A tool-result checkpoint means the branch includes records through that tool result.
- A tool-call checkpoint without a completed result should branch to the state immediately before executing that tool call, represented by an assistant record plus synthetic denied/cancelled tool records for any required missing calls.

Because OpenAI-compatible providers require every assistant tool call to receive a tool message before the next user message, branch creation must repair partial tool-call groups. Reuse or generalize the existing cancellation repair behavior in `src/app.rs` (`finalize_cancelled_turn` and pending tool-call handling) so branched conversations remain valid.

### File edit journal

Add a durable edit journal separate from model-visible conversation records, for example:

```text
~/.cass/file-edits/<chat_id>.jsonl
~/.cass/file-snapshots/<chat_id>/<tool_call_id>/<hash>.bin
```

Journal entries should be written only for successful `write` and `edit` tool calls:

```rust
struct FileEditJournalEntry {
    chat_id: String,
    record_index: usize,
    tool_call_id: String,
    tool_name: String, // write | edit
    path: PathBuf,
    existed_before: bool,
    existed_after: bool,
    before_hash: Option<String>,
    after_hash: Option<String>,
    before_snapshot: Option<PathBuf>,
    after_snapshot: Option<PathBuf>,
    ts: String,
}
```

Implementation approach:

- Add a `file_edits` module that can capture before/after bytes, hash them, store snapshots, append journal entries, and build restore plans.
- Pass chat id / record index / tool call id into tool execution context, or wrap `write`/`edit` execution in `agent.rs` so the agent captures before/after around successful file tools.
- Store full bytes, not just unified diffs, so restore works both backward and forward.
- Limit snapshots to regular files. If a path is a directory, symlink, binary too large, or otherwise unsafe, skip journaling and note that restore will not cover it.

The first implementation can treat text and binary bytes uniformly for snapshot storage, while still using existing `write`/`edit` tools for text operations.

### Restore planning

File restore should compute a target workspace state from the selected checkpoint and branch lineage:

1. Determine the selected checkpoint's branch lineage back to the root conversation.
2. Load file-edit journal entries along that lineage up to the checkpoint.
3. For every path touched by tracked edits in the relevant branch family, compute the desired state at the checkpoint:
   - absent if no tracked edit existed before the checkpoint and the file was created later,
   - the last `after_snapshot` at or before the checkpoint,
   - the `before_snapshot` for paths whose first tracked edit happened after the checkpoint.
4. Compare the current workspace file hash to the journal's expected current hash when possible.
5. Produce a restore plan with actions: write snapshot, delete file, skip unsupported, conflict.
6. Apply only after explicit confirmation.

For v0.2.8, if cross-branch target-state computation becomes too large, keep the algorithm conservative: support full restore for the current branch's lineage and show a clear unsupported/conflict message for unrelated sibling states. The branch metadata should still be designed so broader cross-branch restore can be added later without changing saved data.

### TUI integration

Add branch-menu state to `run_tui` rather than invoking `src/menu.rs` inside the alternate-screen UI. Suggested approach:

- Add an enum such as `OverlayState::BranchMenu(BranchMenuState)` in `src/app.rs` or a new `src/ui/branch_menu.rs`.
- Extend `render::RenderState` to include an optional overlay.
- Render a centered modal with title, help text, visible items, selected row, and preview/detail panel.
- Route key events to the overlay first while it is open.
- On confirmed branch/switch/restore, update:
  - `conversation`,
  - `chat_id`,
  - `transcript = transcript_from_loaded(...)`,
  - active assistant/tool state,
  - scroll/stick-to-bottom/status.

Keep the TUI loop readable by moving branch operations into functions such as:

```rust
open_branch_menu(...)
handle_branch_menu_key(...)
apply_branch_action(...)
```

### Slash command fallback

Optionally add a discoverable slash command such as `/branch` or `/restore` that opens the same menu. This is useful for users whose terminals send unusual `Esc` sequences. If added, document it as an alias for the menu rather than a separate workflow.

## Implementation Steps

1. **Add branch metadata and helpers.** Extend `Record::Meta` compatibly, add branch point types, implement branch-family scanning and `create_branch` from a record prefix.
2. **Build checkpoint extraction.** Convert conversations into user/assistant/tool checkpoints with labels, previews, timestamps, and valid prefix calculations.
3. **Repair tool-call prefixes.** Generalize pending-tool-call repair so branches created around tool calls are valid for future provider requests.
4. **Add edit journaling.** Capture successful `write`/`edit` before/after snapshots, append a file-edit journal entry, and keep this separate from model-visible JSONL records.
5. **Implement restore planning.** Load journal entries, compute target states, detect conflicts by hash, and apply writes/deletes safely with existing atomic-write behavior.
6. **Add the in-TUI menu.** Implement double-`Esc` idle detection, overlay state, rendering, keyboard navigation, action confirmation, and branch/switch application.
7. **Wire status and recovery messages.** Make every branch, switch, restore, skip, and conflict result visible in the transcript or status line.
8. **Document the feature.** Update README and bundled docs with shortcut behavior, branch semantics, file restore coverage, and limitations around shell/manual edits.
9. **Test and polish.** Add unit/integration tests, run formatting, and verify the TUI manually in a small repository.

## Tests

- `conversation` tests:
  - old JSONL conversations without branch metadata still load,
  - new branch metadata serializes/deserializes,
  - `create_branch` leaves the source file unchanged,
  - branch-family scanning finds ancestors and descendants.
- Checkpoint tests:
  - user, assistant, tool-call, and tool-result checkpoints are extracted with stable labels,
  - branching at a tool result keeps valid assistant/tool ordering,
  - branching in the middle of multi-tool assistant output repairs missing tool results.
- File journal tests:
  - `write` records absent-to-present and present-to-present snapshots,
  - `edit` records before/after bytes for successful edits only,
  - failed or denied tools do not create journal entries.
- Restore-plan tests:
  - restore to an earlier checkpoint rewrites tracked files to prior content,
  - restore to a later checkpoint can reapply tracked content from snapshots,
  - created files are deleted only when hashes match,
  - external modifications are reported as conflicts.
- TUI/key tests where practical:
  - first idle `Esc` sets double-press status,
  - second idle `Esc` opens the branch menu,
  - busy `Esc` still cancels a turn,
  - approval `Esc` still denies approval.

Manual checks:

- Start a chat, make a `write` edit, branch conversation-only from before the edit, confirm the original remains available.
- Open the menu from the branch and switch back to the original chat.
- Branch with file restore and verify the workspace file content matches the chosen checkpoint.
- Trigger a conflict by manually editing a tracked file before restore and confirm Cassady refuses to overwrite it by default.

## Documentation

Update:

- `README.md`: everyday workflow section for branching/restoring and a short safety note.
- `docs/commands.md` or the relevant TUI guide: double-`Esc`, optional `/branch`, and menu controls.
- `docs/troubleshooting.md`: conflicts, unsupported shell/manual edits, and how to switch back to the original branch.
- Any keyboard shortcut table maintained in bundled docs.

## Acceptance Criteria

- Pressing `Esc` twice while idle opens a branch/restore menu.
- Selecting a user, assistant, or tool checkpoint creates a new branch conversation without modifying the source conversation.
- The branch menu can be opened from the new branch to switch back to the original or create another branch.
- Users can choose conversation-only branching or branch-plus-file restore.
- File restore covers successful Cassady `write`/`edit` mutations with before/after snapshots and refuses unsafe overwrites by default.
- Branches created around tool calls produce valid future model requests.
- Existing conversations remain loadable.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
