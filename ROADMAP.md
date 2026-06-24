# Cassady (Cass) Roadmap

## v0.2.0 — Control, Context, and Observability

This release focuses on making Cass easier to interrupt, easier to audit, and safer to run on real projects. Large provider expansions and broad protocol integrations are intentionally deferred.

### Agent Control

- [x] **Turn cancellation.** Allow the user to stop a running turn without exiting Cass.
  - While the agent is busy, the first `Ctrl-C` should cancel the active turn.
  - A second `Ctrl-C` can retain the existing exit behavior.
  - You should also be able to press `Esc` to cancel but not exit.
  - Cancellation should stop the provider stream and any in-flight tool execution where possible, including long-running `shell` commands.
  - The conversation should remain resumable after cancellation.

### Context Management

- [x] **Replace message-count trimming with safer context budgeting.** Move away from relying only on `context_message_limit`.
  - Keep provider-reported token usage when available, but do not rely on returned usage as the only pre-request budgeting mechanism.
  - Preserve valid tool-call structure when compacting context; do not leave orphaned tool result messages.
  - Compact or summarize older tool outputs when needed instead of dropping records blindly.

- [x] **Supersede old file-read outputs.** When the same file is read again in a session, avoid repeatedly sending stale large read outputs to the model.
  - Keep the historical tool call visible in the conversation record.
  - Replace superseded model-context output with a short note indicating it was omitted because a newer read exists.
  - Be careful with partial reads: a later read of the same file does not always supersede a different line range.

### Safety and Reviewability

- [x] **Policy-based access control and workspace-edit mode.** Add a central security policy layer and a new `workspace-edit` mode. See `plans/SECURITY_ACCESS_MODES_PLAN.md`.
  - `read-only`: inspect only inside the launch workspace and bundled Cass docs.
  - `workspace-edit`: read and edit files inside the launch workspace without confirmation; bundled Cass docs remain read-only.
  - `workspace-edit`: expose `shell`, but require explicit user confirmation before any command is spawned.
  - `full-access`: preserve broad access while routing decisions through the same policy layer for future restrictions.
  - Refactor tool gating, path authorization, prompt/tool availability, and shell approval through centralized policy decisions instead of scattered `mode.can_write()` checks.
  - Add symlink-aware workspace boundary checks and tests for denied writes outside the workspace.

- [x] **Optional destructive-operation confirmation.** Add a configurable confirmation prompt for risky operations in full-access mode, built on the new policy/approval layer.
  - Cover `write`, `edit`, and `shell` operations that overwrite files or appear destructive.
  - Keep the first version conservative and explicit rather than trying to perfectly classify every shell command.
  - Do not block normal read-only tools.

- [x] **Edit diff output.** Make `edit` changes reviewable in the transcript.
  - First version: show a unified before/after diff after the edit is applied.
  - Later versions may add pre-apply approval, but that requires a confirmation flow between tools and the TUI.
