# Cassady (Cass) Roadmap

## v0.3.2 — Provider Fast Mode

This release focuses on adding a `/fast` command that lets users prefer faster inference when the active provider/model supports it. The first supported provider is `ChatGPT Codex`; other providers can add their own fast-mode request behavior later without changing the user-facing command. See `plans/V0_3_2_FAST_MODE_PLAN.md`.

### Fast Mode Command

- [ ] **Add a persisted `/fast` preference.** Let users toggle fast mode from an idle chat and keep that preference across sessions.
  - Store the preference in `config.json` without disturbing provider/model configuration.
  - Keep the preference separate from whether the current provider/model can honor it.

- [ ] **Show capability-aware fast-mode status.** Display fast mode as enabled only when the active provider/model supports it.
  - If the user switches to an unsupported provider/model, hide the enabled state and report fast mode as unavailable when relevant.
  - If the user switches back to a supported provider/model, apply the existing preference again.

### Provider Support

- [ ] **Implement ChatGPT Codex fast-mode requests.** Add the provider-specific request option for Codex when fast mode is active.
  - Verify and test the exact Codex responses request shape during implementation.
  - Do not send Codex-specific fast-mode fields to OpenAI-compatible providers.

- [ ] **Add extensible provider/model capability metadata.** Model fast-mode support as provider/model metadata so future providers can opt in case by case.
  - Default unknown and custom providers to unsupported.
  - Mark built-in ChatGPT Codex model metadata as supported when Cassady can send the fast-mode request.

### Documentation and Validation

- [ ] **Document fast mode behavior and limits.** Update README and bundled docs for `/fast`, `default_fast_mode`, model capability metadata, and Codex-only initial support.
  - Explain the difference between a saved fast-mode preference and active fast-mode support.

- [ ] **Test fast-mode preference, switching, and provider requests.** Cover command parsing, persistence, status rendering, model/provider switching, and Codex request body behavior.
  - Verify `cargo fmt` and `cargo test --locked --all-targets` pass before handoff.

## v0.3.1 — Transcript Scroll Stability

This release focuses on keeping the live transcript anchored correctly above the input and footer during long sessions with blank reasoning or tool-output lines.

### TUI Reliability

- [x] **Fix bottom-scroll row counting for whitespace-only lines.** Count indented blank transcript rows the same way Ratatui renders them so accumulated blank rows no longer hide recent transcript content above the footer.
  - Add regression coverage for whitespace-only wrapped row counting.

## v0.3.0 — ChatGPT Codex Provider ✅ Completed

This release focuses on letting users who are already signed in to Codex with a ChatGPT subscription use that account from Cassady. `ChatGPT Codex` becomes a provider preset that calls the Codex responses endpoint and reads its bearer token from local Codex auth instead of an API-key environment variable. See `plans/V0_3_0_CHATGPT_CODEX_PROVIDER_PLAN.md`.

### Provider Setup

- [x] **Add a ChatGPT Codex provider preset.** Make `ChatGPT Codex` available from `cass login`, `/login`, and first-run setup as a distinct provider option.
  - Use provider id `chatgpt-codex` and endpoint `https://chatgpt.com/backend-api/codex/responses`.
  - Skip the normal API-key environment-variable prompt for this preset.
  - Prefer the model configured in local Codex config when available, with manual model entry as a fallback.

- [x] **Read local Codex auth safely.** Resolve the bearer token from `$CODEX_HOME/auth.json` or `~/.codex/auth.json` at check/request time.
  - Support the local Codex `tokens.access_token` shape without copying the token into `~/.cass`.
  - Give clear recovery steps when the user has not run `codex login`, signed in to the Codex app, or has an expired/missing token.

### Provider Runtime

- [x] **Add a ChatGPT Codex responses client.** Route `chatgpt-codex` providers to the exact Codex responses endpoint instead of the OpenAI-compatible `/chat/completions` path.
  - Translate Cassady messages, tools, tool calls, and tool outputs to the endpoint's expected responses format.
  - Stream assistant text, safe reasoning summaries when available, and function-call deltas back into the existing agent loop.

- [x] **Keep OpenAI-compatible providers unchanged.** Refactor provider dispatch only as much as needed to support the new provider kind.
  - Existing provider config, setup, model discovery, API-key env vars, `/model`, and `cass check` behavior should continue to work.
  - Avoid leaking ChatGPT access tokens in errors, logs, transcripts, or config files.

### Documentation and Validation

- [x] **Document ChatGPT Codex prerequisites and caveats.** Update README and bundled docs for setup, config examples, `cass check`, troubleshooting, and the distinction between ChatGPT subscription-backed access and API-key providers.
  - Make clear that Cassady uses an existing Codex login and does not implement its own browser login or token refresh flow in this release.
  - Note that the ChatGPT backend endpoint may change outside Cassady's control.

- [x] **Test Codex auth and provider behavior.** Cover Codex auth fixtures, config validation, setup catalog behavior, provider dispatch, streaming response parsing, tool calls, and secret redaction.
  - Verify `cargo fmt` and `cargo test --locked --all-targets` pass before handoff.

## v0.2.9 — Provider Login Management ✅ Completed

This release focuses on making provider configuration available from both the shell and an active Cassady chat. Users can add or update OpenAI-compatible provider/model settings with `cass login` or `/login`, and remove saved providers and their associated models with `cass logout` or `/logout`. See `plans/V0_2_9_PROVIDER_LOGIN_MANAGEMENT_PLAN.md`.

### Login Commands

- [x] **Add login commands for provider setup.** Make `cass login` and `/login` open the provider setup flow so users can configure providers without remembering that setup is the underlying implementation.
  - Reuse the existing provider catalog, model discovery, model capability prompts, and safe JSON writes.
  - Keep `/login` idle-only and reload active provider/model config after the menu closes.

- [x] **Keep existing setup behavior intact.** Preserve `cass setup` and first-run setup while making login language feel natural for account/provider management.
  - Direct `cass login` should save configuration and exit rather than unexpectedly starting a chat.
  - Existing setup validation and missing API key guidance should continue to apply.

### Logout Commands

- [x] **Add a safe provider removal menu.** Make `cass logout` and `/logout` let users choose saved providers to remove from `providers.json`.
  - Remove associated `models.json` entries for selected providers.
  - Confirm the removal before writing changes.

- [x] **Repair active defaults after removal.** Ensure `config.json` never points at a provider/model that was just removed.
  - Select a valid remaining provider/model when possible.
  - Clear active provider/model defaults when no providers remain so the next startup offers login/setup.

### Documentation and Validation

- [x] **Document provider login and logout workflows.** Update command, configuration, and workflow docs with the new shell and in-chat commands.
  - Clarify that logout removes Cassady provider config, not environment variables or external provider accounts.

- [x] **Test provider management behavior.** Cover provider/model removal, active default repair, local command parsing, and autocomplete.
  - Verify `cargo fmt` and `cargo test --locked --all-targets` pass before handoff.

## v0.2.8 — Conversation Branch and Restore ✅ Completed

This release focuses on making conversation recovery safe and explorable. Pressing `Esc` twice while idle opens a branch/restore menu where users can browse prior user messages, assistant messages, and tool calls, create a new branch from a selected checkpoint, and optionally restore Cassady-tracked file edits without destroying the original conversation. See `plans/V0_2_8_CONVERSATION_BRANCH_RESTORE_PLAN.md`.

### Branch Navigation

- [x] **Open a branch/restore menu with double Esc.** Add an idle `Esc`-twice shortcut that mirrors the discoverability of double `Ctrl-C` while preserving current busy-turn cancellation and approval-denial behavior.
  - Keep draft input intact when the menu is opened or cancelled.
  - Provide clear status text after the first `Esc` so users know a second press opens branch/restore.

- [x] **Browse checkpoints across the related branch family.** Show user messages, assistant messages, tool-call requests, and tool results from the current chat and related branches.
  - Include enough preview text, tool names, paths, timestamps, and branch labels to choose the right point.
  - Allow switching back to the original conversation or another existing branch from the same menu.

### Safe Conversation Branching

- [x] **Create branches instead of destructive reverts.** Selecting a checkpoint should write a new conversation JSONL with parent/checkpoint metadata, leaving the source conversation unchanged.
  - Support repeated branching so users can return to the menu later and branch or switch again.
  - Keep older conversations without branch metadata loadable.

- [x] **Handle tool-call checkpoints cleanly.** Branching at a specific tool call or tool result should preserve a valid provider message history.
  - Repair partial multi-tool assistant turns with synthetic cancelled/omitted tool results where needed.
  - Add tests for branching at user, assistant, and tool boundaries.

### File Edit Restoration

- [x] **Journal Cassady file edits with restorable snapshots.** Record successful `write` and `edit` tool mutations outside the model-visible transcript with before/after hashes and snapshots.
  - Keep restore support limited to Cassady-tracked file tools; warn that shell commands and manual edits are not automatically reversible.
  - Store enough data to restore both backward and forward between tracked checkpoints.

- [x] **Offer explicit conversation-only or conversation-plus-files restore actions.** Make conversation-only branching the safe default, and require confirmation before changing workspace files.
  - Preview files to update or delete, detect hash conflicts, and refuse unsafe overwrites by default.
  - Use atomic writes for restored files and preserve clear status/transcript messages for skipped or conflicted paths.

### Documentation and Validation

- [x] **Document branch/restore workflows and limitations.** Update README and bundled docs with the double-`Esc` shortcut, menu controls, branch semantics, and file-restore safety model.
  - Include troubleshooting for restore conflicts and unsupported shell/manual filesystem changes.

- [x] **Test the branch and restore model.** Cover branch metadata, checkpoint extraction, tool-call repair, edit journaling, restore planning, and keybinding behavior where practical.
  - Verify `cargo fmt` and `cargo test --locked --all-targets` pass before release.

## v0.2.7 — Self-Update Command

This release focuses on making Cassady easy to keep current after installation. The goal is to let users run one clean command, `cass update`, to check GitHub releases, choose the recommended prebuilt binary or a source-build fallback, verify what will be installed, and update both `cass` and `cassady` safely. See `plans/V0_2_7_SELF_UPDATE_COMMAND_PLAN.md`.

### Update Command Experience

- [x] **Add a polished `cass update` command.** Check the official Cassady GitHub releases, compare the running version to the selected release, and guide the user through an interactive update flow.
  - Keep update independent of provider/model setup so it works even when config is missing or invalid.
  - Support script-friendly checks with flags such as `--check`, `--dry-run`, and `--yes`.

- [x] **Select the right update path.** Prefer a matching prebuilt release archive when available, with explicit `--prebuilt` and `--source` modes for users who want to choose.
  - Support the same macOS, Linux, and Windows targets used by Cassady releases.
  - Offer source builds for unsupported targets or users who prefer building locally.

### Safe Installation

- [x] **Verify and stage prebuilt artifacts before replacing binaries.** Download archives and SHA-256 files from the release, verify checksums, extract safely, and validate staged `cass`/`cassady` binaries.
  - Reject checksum mismatches, unsafe archive paths, missing binaries, and unexpected versions.
  - Show clear progress and failure messages without dumping raw implementation details.

- [x] **Replace installed binaries cleanly.** Update the current binary and same-directory companion binary when possible, using backups and rollback on failure.
  - Do not auto-run `sudo` or administrator prompts.
  - Handle Windows executable replacement with a staged helper or documented manual fallback if necessary.

### Source Build Fallback and Documentation

- [x] **Build from release source when requested.** Download the selected release source, validate its version, check for Rust tooling, run a locked release build, and install the resulting local binaries.
  - Keep source mode tied to release tags rather than arbitrary branches.
  - Do not attempt cross-compilation or Rust toolchain installation in this release.

- [x] **Document and test update behavior.** Update README and bundled docs for `cass update`, platform notes, troubleshooting, and package-manager caveats.
  - Add tests for release parsing, target mapping, asset selection, checksum validation, archive extraction safety, install planning, and mocked update flows.

## v0.2.6 — Rust Embedding API ✅ Completed

This release focuses on adding the first intentional Rust library surface for embedding Cassady in other Rust projects. The goal is to provide the bones for programmatic, headless agent sessions: configure a workspace, start or resume an agent session, send turns, stream typed events, and handle approvals without launching the TUI. See `plans/V0_2_6_RUST_EMBEDDING_API_PLAN.md`.

### Experimental Public API

- [x] **Add a supported embedding module.** Provide a small `cassady::embedding` API with builder, session, turn, event, approval, and error types so callers do not need to stitch together internal modules directly.
  - Mark the API experimental for v0.2.6 rather than promising long-term semver stability.
  - Keep existing CLI/TUI behavior unchanged while steering library users toward the new module.

- [x] **Support host-configured agent sessions.** Let Rust callers create or resume headless sessions with explicit cwd, access mode, model/provider overrides, reasoning effort, and Cassady config root.
  - Reuse existing config files, global instructions, bundled docs, security policy, and JSONL conversation storage.
  - Avoid requiring callers to construct CLI-specific types.

### Headless Turn Execution

- [x] **Run agent turns programmatically.** Add a Tokio-native API for sending one user message, streaming assistant/tool/status events, and returning the updated session or conversation state.
  - Prevent or clearly reject overlapping turns unless the type design makes them impossible.
  - Preserve provider streaming, tool execution, prompt generation, and context behavior from the existing agent loop.

- [x] **Expose approval handling to host applications.** Allow embedded callers to approve or deny tool approval requests, especially shell commands in `workspace-edit` mode.
  - Include request id, tool call id, tool name, arguments, and reason in approval events.
  - Document cancellation/drop behavior for active turns.

### Documentation and Validation

- [x] **Add a minimal headless example.** Include a compilable Rust example that imports Cassady, starts a session, sends a prompt, and prints streamed assistant output.
  - Note that a configured OpenAI-compatible provider and API key are still required.
  - Show where to handle approval requests even if the first example defaults to `read-only`.

- [x] **Document the experimental Rust API.** Add bundled docs and README links for setup requirements, basic usage, event handling, approvals, limitations, and current non-goals.
  - Make clear that multi-agent orchestration, custom providers, custom tools, daemons, and stable plugin APIs are deferred.

- [x] **Test embedding without a terminal.** Add integration tests that use temporary config/conversation roots and mock provider responses to verify session creation, turn streaming, resume, approval flow, and access-mode behavior.
  - Ensure `cargo test --locked --all-targets` covers the new public API and examples.

## v0.2.4 — System Prompt Refinement

This release focuses on making Cassady's system prompt clearer, more intuitive, and more useful for everyday coding work without letting it become bulky. The target is a well-structured prompt around 1,000 tokens that gives the model enough product context, safety expectations, and workflow guidance to behave consistently across read-only, workspace-edit, and full-access sessions. See `plans/V0_2_4_SYSTEM_PROMPT_REFINEMENT_PLAN.md`.

### Prompt Structure and Content

- [x] **Restructure the prompt into clear sections.** Replace the current compact prompt with a polished, scannable structure that explains identity, operating principles, tool use, editing, safety, and response style in a predictable order.
  - Keep headings short and model-friendly so the prompt is easy to follow during long sessions.
  - Preserve the existing split between the reusable base prompt, user global instructions, and runtime constraints.
  - Avoid duplicating long documentation that already lives in README or bundled docs.

- [x] **Add enough product context for intuitive behavior.** Teach the model what Cassady is, how the terminal chat works, and what the user can see without over-explaining implementation details.
  - Explain that tool calls, tool results, diffs, approvals, and streamed assistant text are visible in the transcript.
  - Clarify that Cassady is a coding assistant for real project work, so it should inspect before changing files, make targeted edits, and summarize outcomes honestly.
  - Include guidance for asking focused follow-up questions only when necessary, instead of over-planning or guessing.

- [x] **Keep the prompt intentionally compact.** Aim for roughly 900-1,100 tokens for the normal effective system prompt, including runtime access-mode guidance but excluding user-provided global instructions.
  - Prefer dense, high-signal instructions over broad lists of examples.
  - Remove redundant wording when new guidance overlaps with existing safety or response rules.
  - Add a lightweight test or snapshot check so future prompt changes do not accidentally grow far beyond the intended size.

### Tool, Editing, and Safety Guidance

- [x] **Improve tool-use instructions.** Make the prompt explicit about when to read, grep, edit, write, and shell while still letting the model choose the right tool for the task.
  - Encourage targeted inspection before edits and `grep`/search before opening large or unknown files.
  - Explain that the model should request tools directly when useful; Cassady will enforce access policy, denials, and approval prompts at runtime.
  - Remind the model not to claim a tool succeeded until the tool result confirms it.

- [x] **Sharpen editing guidance.** Make file-change behavior safer and more reliable, especially for exact-text edits.
  - Prefer `edit` for focused modifications and `write` only for new files or intentional full rewrites.
  - Instruct the model to keep replacements minimal, unique, and non-overlapping.
  - Encourage running or suggesting relevant tests after meaningful code changes.

- [x] **Make access-mode behavior easy for the model to follow.** Rewrite read-only, workspace-edit, and full-access guidance in plain language that maps directly to available tools.
  - Keep workspace and bundled-doc boundaries clear.
  - State that shell approval is handled by Cassady's UI rather than by asking for permission in chat.
  - Preserve conservative behavior when a task requires permissions the current mode does not allow.

### Validation and Documentation

- [x] **Add prompt-focused tests.** Verify that the generated prompt includes the required sections, preserves global instructions, reflects the active access mode, and stays within the intended size range.
  - Cover read-only, workspace-edit, and full-access effective prompts.
  - Include a regression check for prompt ordering so runtime constraints remain near the end.

- [x] **Update user-facing references to global instructions.** Refresh docs only where needed to explain how `~/.cass/global.md` fits into the structured prompt.
  - Avoid exposing the full internal prompt in documentation.
  - Mention that user global instructions are respected unless they conflict with runtime safety constraints.

## v0.2.3 — Documentation and README Refresh ✅ Completed

This release focuses on making Cassady understandable, trustworthy, and easy to operate by rewriting the README and bringing all bundled documentation up to date with the current CLI behavior. The work should cover user-facing documentation only; broad CLI feature work and Windows-specific runtime improvements are deferred to the planned Windows CLI usability work. See `plans/V0_2_3_DOCUMENTATION_README_REFRESH_PLAN.md`.

### README Rewrite

- [x] **Rewrite the README around the current Cassady experience.** Replace stale or incomplete sections with a clear, accurate guide to what Cassady is, who it is for, and how to start using it.
  - Add a concise product summary, core capabilities, supported workflows, and current limitations.
  - Document both `cass` and `cassady` command names where relevant.
  - Keep examples aligned with the current setup wizard, active provider/model configuration, access modes, tools, and TUI behavior.
  - Remove outdated MVP language, obsolete commands, and instructions that no longer match the code.

- [x] **Add a complete first-use walkthrough.** Make the README guide a new user from launching the CLI to a successful first chat without requiring them to infer missing steps.
  - Cover first-run setup, `cass setup`, `cass check`, provider selection, API key environment variables, model discovery, and manual model entry fallback.
  - Include copy/paste-ready examples for common providers without exposing secrets or implying one provider is required.
  - Explain what happens when setup is incomplete and how the user should recover.

- [x] **Document everyday workflows.** Add practical examples for the CLI actions users are most likely to perform after setup.
  - Starting a chat in a project workspace.
  - Asking Cassady to inspect files, explain code, propose edits, and apply edits.
  - Reviewing tool calls, collapsed tool output, edit diffs, and assistant Markdown rendering.
  - Cancelling a turn, recovering after cancellation, and exiting cleanly.

### Reference Documentation

- [x] **Create or refresh the CLI command reference.** Document all supported commands, flags, aliases, and expected output modes in one place.
  - Include `cass`, `cassady`, `setup`, `check`, chat startup behavior, config overrides, access-mode flags, and any non-interactive/script-friendly commands.
  - Show when commands are interactive versus non-interactive.
  - Keep examples shell-neutral where possible, and label platform-specific syntax when needed.

- [x] **Rewrite the configuration reference.** Explain where config lives, how active provider/model selection works, and how users should safely edit or validate config.
  - Document provider entries, model entries, active defaults, API key environment variable names, custom OpenAI-compatible providers, and model IDs.
  - Explain precedence between config files, environment variables, command-line overrides, and setup wizard changes.
  - Include valid example config snippets and common invalid configurations with fixes.

- [x] **Document providers and model setup thoroughly.** Add a dedicated guide for built-in OpenAI-compatible providers and custom provider setup.
  - List supported built-in providers, base URLs, expected API key env vars, and any known model-discovery limitations.
  - Explain the difference between provider configuration, model selection, and API key availability.
  - Document manual model entry, provider health checks, and how `cass check` reports provider problems.
  - Explicitly note protocols or providers that are not supported yet to avoid user confusion.

- [x] **Document access modes and tool safety.** Make the security model understandable before users let Cassady inspect or edit a repository.
  - Explain `read-only`, `workspace-edit`, and `full-access` in user-facing terms.
  - Document what read, write, edit, shell, and bundled-doc access mean in each mode.
  - Explain shell approval prompts, optional destructive-operation confirmation, workspace boundaries, symlink handling, and edit diff review.
  - Add examples of denied operations and the exact kind of message a user should expect.

### Usage Guides and Troubleshooting

- [x] **Add troubleshooting for common failure modes.** Give users actionable fixes for the errors they are most likely to hit.
  - Missing API keys, invalid env vars, unreachable provider URLs, model discovery failures, unsupported model IDs, and rate/authentication errors.
  - Broken or incomplete config, unreadable config paths, invalid TOML/JSON if applicable, and permission problems.
  - Terminal rendering issues, non-interactive terminals, redirected output, shell command failures, and cancellation behavior.
  - File edit failures, failed exact-text replacements, binary/large files, line ending issues, and workspace access denials.

- [x] **Add task-oriented examples.** Include short, realistic examples that demonstrate how Cassady should be used on real projects.
  - Code explanation and navigation.
  - Safe file editing with diff review.
  - Running tests or build commands with shell approval.
  - Updating config or switching providers/models.
  - Resuming work after a failed provider request or cancelled turn.

- [x] **Document platform expectations without duplicating future Windows work.** Add accurate notes for macOS, Linux, and Windows users while keeping deep Windows CLI usability improvements scoped to the planned Windows CLI usability work.
  - Include path, shell, and environment-variable examples for each platform when documentation needs them.
  - Mark known Windows limitations clearly until the planned Windows CLI usability work lands.
  - Avoid promising installer, package manager, or auto-update behavior that is not implemented.

### Documentation Quality and Maintenance

- [x] **Improve prose quality and readability.** Rewrite docs in clear sentences and paragraphs instead of relying on long, list-heavy outlines.
  - Use bullets and tables only when they improve scanning, such as setup steps, command references, provider lists, or troubleshooting checklists.
  - Prefer short explanatory paragraphs for concepts, workflows, tradeoffs, and safety guidance.
  - Avoid turning every section into nested bullets; the README should feel like polished documentation, not an implementation checklist.

- [x] **Synchronize README, bundled docs, and CLI help text.** Ensure every user-facing description of commands, modes, providers, setup, and tools says the same thing.
  - Audit README, docs, inline help, setup prompts, `cass check` guidance, and release notes templates for contradictions.
  - Update terminology consistently: Cassady/Cass, provider, model, workspace, access mode, tool call, tool result, and session.
  - Make sure future docs can be updated from one source of truth where practical.

- [x] **Improve documentation structure and navigation.** Make the docs easy to scan and hard to misuse.
  - Add a table of contents or clear section links where the document is long.
  - Move long reference material out of the README when it distracts from first-use guidance, and link to it clearly.
  - Add a glossary for recurring concepts such as workspace, provider, model, access mode, context, and tool call.
  - Ensure headings, examples, and filenames follow a consistent style.

- [x] **Verify documentation against the actual CLI.** Treat docs as tested user experience, not prose written from memory.
  - Run the documented commands and update examples to match real output.
  - Check every internal link, file path, command name, provider URL, env var, and config snippet.
  - Add lightweight docs checks where practical, such as link validation or command-output smoke tests.
  - Complete the release only when README and bundled docs accurately reflect the shipped CLI.

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

## Planned within the next major release

These sections describe work Cassady intends to complete before or as part of the next major release, but which has not yet been assigned to a specific version. Scope, order, and version numbers may change.

### Tool Output Context Reliability

- [ ] **Reduce tool-output compaction stalls.** Make large file reads and command output easier to recover from when output is compacted or truncated, so the assistant can quickly switch to targeted inspection instead of getting stuck.
  - Prefer smaller, focused file ranges and search-first workflows when large outputs are likely.
  - Surface clearer guidance when tool results are compacted, including suggested narrower follow-up reads.
  - Add regression coverage or dogfood checks for workflows where broad reads previously obscured the context needed for safe edits.

### Windows CLI Usability

This work focuses on making Cassady feel reliable and native when the CLI is run on Windows. It covers runtime usability after `cass` or `cassady` is already available on the machine; installers, package managers, PATH setup, code signing, and update delivery are intentionally out of scope.

#### Terminal Experience

- [ ] **Make interactive rendering robust in Windows terminals.** Ensure chat, setup, confirmation prompts, streamed output, spinners, diffs, and tool summaries render cleanly in Windows Terminal, PowerShell, Command Prompt, and common VS Code integrated terminals.
  - Enable or gracefully detect ANSI/VT support instead of emitting broken escape sequences.
  - Respect `NO_COLOR`, non-interactive output, redirected stdout/stderr, and narrow terminal widths.
  - Avoid relying on glyphs, emoji, box drawing, or cursor control sequences that render poorly on default Windows fonts.
  - Keep wrapping and cursor positioning correct for multi-line input, Markdown output, and long tool-call summaries.

- [ ] **Harden keyboard handling on Windows.** Make the TUI and prompts respond predictably to Windows console input events.
  - Verify `Enter`, `Backspace`, `Delete`, arrow keys, `Home`, `End`, `PageUp`, `PageDown`, `Tab`, and paste behavior.
  - Preserve existing `Ctrl-C` cancellation semantics and handle `Ctrl-Break`/console close events gracefully where supported.
  - Ensure `Esc` cancellation and prompt dismissal work consistently across PowerShell, Command Prompt, and Windows Terminal.

- [ ] **Improve plain CLI output for Windows users.** Commands such as `cass check`, setup diagnostics, validation errors, and usage text should remain readable without a fully interactive terminal.
  - Prefer actionable Windows examples using PowerShell syntax when the current platform is Windows.
  - Avoid POSIX-only command snippets in runtime guidance unless explicitly labeled.
  - Keep error messages copy/paste-friendly and free of terminal control characters when output is redirected.

#### Windows Paths and Files

- [ ] **Support Windows path syntax everywhere the CLI accepts paths.** Normalize and validate paths consistently across arguments, tool calls, diffs, session metadata, and model-visible file references.
  - Handle drive-letter paths such as `C:\Users\name\project`, rooted paths such as `\temp`, UNC paths such as `\\server\share\repo`, and mixed `/`/`\` separators.
  - Preserve user-facing paths in a readable Windows form while using canonicalized paths for safety decisions.
  - Avoid treating `:` in drive letters as URL schemes or command separators.
  - Add tests for relative path resolution from Windows workspaces and for paths containing spaces, apostrophes, parentheses, brackets, and non-ASCII characters.

- [ ] **Respect Windows filesystem semantics in workspace policy.** Keep read, write, edit, and shell safety checks correct on NTFS and common Windows filesystems.
  - Account for case-insensitive path comparisons, symlinks, junctions, directory symlinks, and network shares.
  - Prevent workspace escapes through `..`, junctions, symlink targets, alternate path spellings, and UNC aliases.
  - Handle reserved device names, trailing dots/spaces, invalid filename characters, and long-path edge cases with clear errors.
  - Preserve current access modes (`read-only`, `workspace-edit`, `full-access`) with Windows-specific authorization tests.

- [ ] **Handle line endings and encodings cleanly.** Make file reads, edits, diffs, and generated files predictable on Windows projects.
  - Preserve existing CRLF/LF style when editing files where practical.
  - Render diffs clearly even when files use CRLF line endings.
  - Avoid corrupting UTF-8 with BOM, UTF-16, or non-UTF-8 files; detect unsupported text encodings and explain the limitation.
  - Keep binary-file detection reliable for Windows executables, images, archives, and generated build artifacts.

#### Shell and Process Integration

- [ ] **Use the right shell behavior on Windows.** Make `shell` tool execution, approval prompts, command summaries, cancellation, and exit status reporting work with Windows process semantics.
  - Prefer PowerShell-friendly examples and diagnostics while still supporting `cmd.exe`-style commands when users provide them.
  - Quote paths with spaces safely and avoid POSIX-only escaping in Windows-generated commands.
  - Surface the actual executable, working directory, exit code, stdout, and stderr in a way users can debug.
  - Cancel long-running child processes cleanly, including process trees where possible.

- [ ] **Normalize environment-variable handling.** Ensure provider API key checks, diagnostics, setup guidance, and spawned tools work with Windows environment conventions.
  - Treat environment variable names consistently despite Windows case-insensitive lookup behavior.
  - Show PowerShell examples such as `$env:OPENAI_API_KEY = "..."` for temporary values.
  - Avoid relying on POSIX shell expansion, `export`, `$VAR`, or `~` in Windows-specific guidance.

- [ ] **Support common Windows external commands and editors.** When Cassady suggests or launches helper commands, make the behavior compatible with typical Windows environments.
  - Detect missing tools and explain alternatives rather than assuming Unix utilities are present.
  - Avoid hard dependencies on `sh`, `bash`, `grep`, `sed`, `cat`, `less`, or `/tmp` during normal CLI operation.
  - Respect configured editor/browser commands and quote file paths correctly when opening files or URLs.

#### Config, State, and Session Usability

- [ ] **Use Windows-appropriate runtime locations.** Keep config, logs, caches, sessions, temporary files, and diagnostics in locations that align with Windows conventions.
  - Prefer the existing cross-platform directory abstraction where available, and verify behavior with `APPDATA`, `LOCALAPPDATA`, `TEMP`, and `USERPROFILE`.
  - Expand `~` and environment-derived paths consistently in config values.
  - Keep session history portable enough to display Windows paths without breaking transcript replay.

- [ ] **Make diagnostics expose Windows-specific context.** Improve `cass check` and error reports so Windows users can understand terminal, filesystem, shell, and config problems quickly.
  - Include OS, architecture, terminal detection, active shell, config path, workspace path, and access mode when relevant.
  - Clearly distinguish provider/API-key failures from Windows runtime issues.
  - Recommend Windows-native remediation steps without mentioning installation tasks.

- [ ] **Keep aliases and command parsing consistent.** Ensure `cass` and `cassady` subcommands, flags, config overrides, and path arguments behave the same on Windows as on Unix-like systems.
  - Validate quoting behavior for arguments containing spaces and backslashes.
  - Ensure help text and examples do not imply shell features unavailable in PowerShell or Command Prompt.
  - Keep machine-readable output stable across platforms when output is consumed by scripts.

#### Verification and Documentation

- [ ] **Add Windows-focused automated coverage.** Add unit and integration tests that exercise Windows path parsing, policy checks, config discovery, line endings, environment variables, and command rendering.
  - Use platform-gated tests for behavior that can only run on Windows.
  - Add platform-independent tests for Windows path strings where possible.
  - Include regression tests for spaces in paths, UNC paths, CRLF edits, and workspace escape attempts.

- [ ] **Run a manual Windows CLI acceptance pass.** Validate the release on a real Windows environment, not just cross-compilation.
  - Test PowerShell, Command Prompt, Windows Terminal, and VS Code integrated terminal.
  - Exercise interactive chat, first-run setup, `cass check`, tool approvals, file read/edit/diff, shell cancellation, and redirected output.
  - Record any unsupported terminal or shell behavior as explicit known limitations.

- [ ] **Update runtime documentation for Windows usage.** Refresh README and bundled docs with Windows-specific CLI usage guidance while avoiding installation instructions.
  - Document PowerShell environment-variable examples, path examples, terminal expectations, and known limitations.
  - Include troubleshooting for broken colors, bad wrapping, path authorization failures, CRLF diffs, and missing Unix helper commands.
  - Keep all Windows guidance consistent with existing access modes and safety policies.
