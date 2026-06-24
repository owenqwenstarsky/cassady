# Cassady (Cass) Roadmap

## v0.2.2 — First-Run Onboarding and Setup Wizard ✅ Completed

This release focuses on making Cassady easy to start using from a fresh install. See `plans/V0_2_2_ONBOARDING_SETUP_WIZARD_PLAN.md`.

### Interactive Setup

- [x] **Add a first-run setup wizard.** When `cass` cannot resolve a usable active provider/model/API key, guide the user through setup instead of starting a chat that will fail.
  - Trigger automatically on first run or incomplete setup.
  - Add `cass setup` to run the wizard explicitly.
  - Keep `cass check` non-interactive.

- [x] **Support OpenAI-compatible provider selection.** Present a reusable keyboard menu with multi-select built-in providers and a custom provider option.
  - OpenAI: `https://api.openai.com/v1`
  - xAI: `https://api.x.ai/v1`
  - Fireworks: `https://api.fireworks.ai/inference/v1`
  - Groq: `https://api.groq.com/openai/v1`
  - OpenRouter: `https://openrouter.ai/api/v1`
  - OpenCode Zen: `https://opencode.ai/zen/v1`
  - OpenCode Go: `https://opencode.ai/zen/go/v1`
  - Cerebras: `https://api.cerebras.ai/v1`
  - Novita: `https://api.novita.ai/v3/openai`
  - Together: `https://api.together.xyz/v1`
  - Custom OpenAI-compatible provider.
  - Do not add Anthropic-native or non-OpenAI-compatible protocols in this release.

- [x] **Guide API key configuration.** Default to environment-variable based API keys, check whether the selected env var is set, and provide exact next steps when it is missing.

- [x] **Guide first-model selection.** Attempt OpenAI-compatible `GET /models` discovery when the API key is available, allow selecting a discovered model, and always provide manual model id entry as a fallback.

- [x] **Write valid config and start the first session.** Upsert provider/model entries, set active defaults, run setup validation, and automatically start a new chat only when the active API key is available.

### Setup Diagnostics and Docs

- [x] **Improve `cass check` onboarding output.** Show active provider, base URL, model, API key env var status, and actionable next steps such as `cass setup` or `export PROVIDER_API_KEY=...`.

- [x] **Refresh onboarding documentation.** Update README and bundled docs for `cass setup`, first-run behavior, OpenAI-compatible provider selection, API key env vars, model discovery/manual fallback, and current access modes including `workspace-edit`.

## v0.2.1 — Message Rendering Polish ✅ Completed

### Transcript Rendering

- [x] **Markdown message rendering and cleaner tool-call display.** Render assistant and user message blocks as Markdown, and improve tool call rendering/display so tool invocations and results are easier to scan in the transcript.
  - Completed tool invocation/processing blocks are removed once the result arrives.
  - Collapsed successful tool results render as a one-line summary.
  - Successful `ls` results are hidden in collapsed mode to avoid transcript clutter; full tool view still shows them.

## v0.2.0 — Control, Context, and Observability ✅ Completed

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
