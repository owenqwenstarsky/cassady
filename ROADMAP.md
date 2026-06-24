# Cassady (Cass) Roadmap

## v0.2.4 — Windows CLI Usability

This release focuses on making Cassady feel reliable and native when the CLI is run on Windows. It covers runtime usability after `cass` or `cassady` is already available on the machine; installers, package managers, PATH setup, code signing, and update delivery are intentionally out of scope.

### Terminal Experience

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

### Windows Paths and Files

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

### Shell and Process Integration

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

### Config, State, and Session Usability

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

### Verification and Documentation

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

## v0.2.3 — Documentation and README Refresh

This release focuses on making Cassady understandable, trustworthy, and easy to operate by rewriting the README and bringing all bundled documentation up to date with the current CLI behavior. The work should cover user-facing documentation only; broad CLI feature work and Windows-specific runtime improvements are deferred to v0.2.4.

### README Rewrite

- [ ] **Rewrite the README around the current Cassady experience.** Replace stale or incomplete sections with a clear, accurate guide to what Cassady is, who it is for, and how to start using it.
  - Add a concise product summary, core capabilities, supported workflows, and current limitations.
  - Document both `cass` and `cassady` command names where relevant.
  - Keep examples aligned with the current setup wizard, active provider/model configuration, access modes, tools, and TUI behavior.
  - Remove outdated MVP language, obsolete commands, and instructions that no longer match the code.

- [ ] **Add a complete first-use walkthrough.** Make the README guide a new user from launching the CLI to a successful first chat without requiring them to infer missing steps.
  - Cover first-run setup, `cass setup`, `cass check`, provider selection, API key environment variables, model discovery, and manual model entry fallback.
  - Include copy/paste-ready examples for common providers without exposing secrets or implying one provider is required.
  - Explain what happens when setup is incomplete and how the user should recover.

- [ ] **Document everyday workflows.** Add practical examples for the CLI actions users are most likely to perform after setup.
  - Starting a chat in a project workspace.
  - Asking Cassady to inspect files, explain code, propose edits, and apply edits.
  - Reviewing tool calls, collapsed tool output, edit diffs, and assistant Markdown rendering.
  - Cancelling a turn, recovering after cancellation, and exiting cleanly.

### Reference Documentation

- [ ] **Create or refresh the CLI command reference.** Document all supported commands, flags, aliases, and expected output modes in one place.
  - Include `cass`, `cassady`, `setup`, `check`, chat startup behavior, config overrides, access-mode flags, and any non-interactive/script-friendly commands.
  - Show when commands are interactive versus non-interactive.
  - Keep examples shell-neutral where possible, and label platform-specific syntax when needed.

- [ ] **Rewrite the configuration reference.** Explain where config lives, how active provider/model selection works, and how users should safely edit or validate config.
  - Document provider entries, model entries, active defaults, API key environment variable names, custom OpenAI-compatible providers, and model IDs.
  - Explain precedence between config files, environment variables, command-line overrides, and setup wizard changes.
  - Include valid example config snippets and common invalid configurations with fixes.

- [ ] **Document providers and model setup thoroughly.** Add a dedicated guide for built-in OpenAI-compatible providers and custom provider setup.
  - List supported built-in providers, base URLs, expected API key env vars, and any known model-discovery limitations.
  - Explain the difference between provider configuration, model selection, and API key availability.
  - Document manual model entry, provider health checks, and how `cass check` reports provider problems.
  - Explicitly note protocols or providers that are not supported yet to avoid user confusion.

- [ ] **Document access modes and tool safety.** Make the security model understandable before users let Cassady inspect or edit a repository.
  - Explain `read-only`, `workspace-edit`, and `full-access` in user-facing terms.
  - Document what read, write, edit, shell, and bundled-doc access mean in each mode.
  - Explain shell approval prompts, optional destructive-operation confirmation, workspace boundaries, symlink handling, and edit diff review.
  - Add examples of denied operations and the exact kind of message a user should expect.

### Usage Guides and Troubleshooting

- [ ] **Add troubleshooting for common failure modes.** Give users actionable fixes for the errors they are most likely to hit.
  - Missing API keys, invalid env vars, unreachable provider URLs, model discovery failures, unsupported model IDs, and rate/authentication errors.
  - Broken or incomplete config, unreadable config paths, invalid TOML/JSON if applicable, and permission problems.
  - Terminal rendering issues, non-interactive terminals, redirected output, shell command failures, and cancellation behavior.
  - File edit failures, failed exact-text replacements, binary/large files, line ending issues, and workspace access denials.

- [ ] **Add task-oriented examples.** Include short, realistic examples that demonstrate how Cassady should be used on real projects.
  - Code explanation and navigation.
  - Safe file editing with diff review.
  - Running tests or build commands with shell approval.
  - Updating config or switching providers/models.
  - Resuming work after a failed provider request or cancelled turn.

- [ ] **Document platform expectations without duplicating future Windows work.** Add accurate notes for macOS, Linux, and Windows users while keeping deep Windows CLI usability improvements scoped to v0.2.4.
  - Include path, shell, and environment-variable examples for each platform when documentation needs them.
  - Mark known Windows limitations clearly until the v0.2.4 work lands.
  - Avoid promising installer, package manager, or auto-update behavior that is not implemented.

### Documentation Quality and Maintenance

- [ ] **Improve prose quality and readability.** Rewrite docs in clear sentences and paragraphs instead of relying on long, list-heavy outlines.
  - Use bullets and tables only when they improve scanning, such as setup steps, command references, provider lists, or troubleshooting checklists.
  - Prefer short explanatory paragraphs for concepts, workflows, tradeoffs, and safety guidance.
  - Avoid turning every section into nested bullets; the README should feel like polished documentation, not an implementation checklist.

- [ ] **Synchronize README, bundled docs, and CLI help text.** Ensure every user-facing description of commands, modes, providers, setup, and tools says the same thing.
  - Audit README, docs, inline help, setup prompts, `cass check` guidance, and release notes templates for contradictions.
  - Update terminology consistently: Cassady/Cass, provider, model, workspace, access mode, tool call, tool result, and session.
  - Make sure future docs can be updated from one source of truth where practical.

- [ ] **Improve documentation structure and navigation.** Make the docs easy to scan and hard to misuse.
  - Add a table of contents or clear section links where the document is long.
  - Move long reference material out of the README when it distracts from first-use guidance, and link to it clearly.
  - Add a glossary for recurring concepts such as workspace, provider, model, access mode, context, and tool call.
  - Ensure headings, examples, and filenames follow a consistent style.

- [ ] **Verify documentation against the actual CLI.** Treat docs as tested user experience, not prose written from memory.
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
