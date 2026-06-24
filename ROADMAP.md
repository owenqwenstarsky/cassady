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

- [ ] **Supersede old file-read outputs.** When the same file is read again in a session, avoid repeatedly sending stale large read outputs to the model.
  - Keep the historical tool call visible in the conversation record.
  - Replace superseded model-context output with a short note indicating it was omitted because a newer read exists.
  - Be careful with partial reads: a later read of the same file does not always supersede a different line range.

### Usage and Cost Visibility

- [ ] **Track provider-reported usage.** Capture input/output token usage returned by providers when available.
  - Support streaming usage metadata for OpenAI-compatible providers where supported.
  - Persist usage information with the conversation when possible.
  - Show useful per-turn or session usage in the UI/status output.

- [ ] **Track cost when available.** Display cost in the footer or status when the provider returns cost directly.
  - If provider cost is unavailable, show token usage rather than estimating silently.
  - Add explicit pricing metadata later if estimated costs are needed.

### Safety and Reviewability

- [ ] **Optional destructive-operation confirmation.** Add a configurable confirmation prompt for risky operations in full-access mode.
  - Cover `write`, `edit`, and `shell` operations that overwrite files or appear destructive.
  - Keep the first version conservative and explicit rather than trying to perfectly classify every shell command.
  - Do not block normal read-only tools.

- [ ] **Edit diff output.** Make `edit` changes reviewable in the transcript.
  - First version: show a unified before/after diff after the edit is applied.
  - Later versions may add pre-apply approval, but that requires a confirmation flow between tools and the TUI.
