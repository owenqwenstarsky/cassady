# Cassady (Cass) Implementation Plan

Implementation status: v1 MVP implemented in this repository. It includes the Rust crate, `cass` and `cassady` binaries, JSONL persistence, read-only/full-access modes, `ls`/`read`/`grep`/`write`/`edit` tools, atomic writes for mutating tools, OpenAI-compatible provider support with Fireworks defaults, and a ratatui/crossterm chat UI.

## 1. Goal and Scope

Cassady, or Cass, is a minimal Rust terminal coding agent with a looped chat interface. The installed command should be available as both `cass` and `cassady`. The interface lets a user type messages, send them to an LLM-backed agent, and receive streamed assistant output.

Cass starts in read-only mode by default. In read-only mode, filesystem inspection is restricted to the directory where the CLI was launched, or to the directory supplied with `--cwd`. Cass can be toggled into full-access mode by pressing `Shift-Tab`, but only while the agent is idle and not processing a turn. In full-access mode, Cass may access normal filesystem paths available to the user and may use mutating tools.

The agent stores configuration, global instructions, and conversations under `~/.cass/`. Conversations are persisted as JSONL so a previous chat can be resumed with `--resume <chat-id>`. Running `--resume` without a chat id lists past chats for the current directory. Stored conversation data should contain only the data needed to reconstruct context: system prompt, user messages, assistant messages, tool calls, and tool results.

Cass ships with a deliberately small filesystem tool set: `ls`, `read`, `grep`, `write`, and `edit`. In read-only mode, only `ls`, `read`, and `grep` are available. In full-access mode, all tools are available. There is no shell/bash command tool in v1.

## 2. Product Behavior

The CLI opens into an interactive terminal chat session. It shows the current chat id, current access mode, working directory, and a single user input area. The UI should feel clean and modern without becoming a complex full IDE-like TUI.

The default access mode is read-only. Pressing `Shift-Tab` toggles between read-only and full-access mode only when Cass is idle. If the model is currently streaming, executing tools, or otherwise processing a user turn, `Shift-Tab` should do nothing or show a small status hint such as `mode can be changed when idle`.

Exiting requires pressing `Ctrl-C` twice within 1.5 seconds. The first `Ctrl-C` clears the current user input box and shows a short hint that another `Ctrl-C` will exit. If the second `Ctrl-C` is not received within 1.5 seconds, the exit intent expires and later `Ctrl-C` presses again only clear the input. When Cass exits a chat this way, it should print a resume hint after restoring the terminal, exactly in this shape: `Resume this chat with: cass --resume <id>`.

During a turn, Cass sends the conversation context and system prompt to the model. Assistant message blocks stream line by line as they arrive. Tool calls are displayed live, including tool name and arguments. When each tool result arrives, Cass displays the result beneath the corresponding tool call. Tool results are truncated by default; pressing `Ctrl-O` toggles between truncated and full tool-call/tool-result display.

## 3. Project Structure

Create a small Rust workspace or single crate with a focused module layout.

```text
cass/
  Cargo.toml
  README.md
  PLAN.md
  src/
    main.rs
    cli.rs
    app.rs
    config.rs
    conversation.rs
    agent.rs
    prompt.rs
    access.rs
    error.rs
    tools/
      mod.rs
      schema.rs
      ls.rs
      read.rs
      grep.rs
      write.rs
      edit.rs
      path.rs
    ui/
      mod.rs
      terminal.rs
      events.rs
      render.rs
      theme.rs
    providers/
      mod.rs
      types.rs
      openai_compatible.rs
  tests/
    conversation_tests.rs
    tool_read_tests.rs
    tool_grep_tests.rs
    tool_edit_tests.rs
    access_mode_tests.rs
    fixtures/
```

Keep modules focused: conversation persistence should not know about terminal rendering, tools should not know about model providers, and the agent loop should coordinate model calls, tool execution, streaming events, and persistence.

## 4. Rust Libraries

Use stable Rust, preferably the latest stable toolchain.

Recommended runtime libraries:

- `tokio` for async runtime, streaming model responses, and non-blocking UI coordination.
- `ratatui` plus `crossterm` for a clean terminal UI with panes, status bars, key handling, and rendering control.
- `tui-textarea` or a small custom input widget for multiline input editing.
- `clap` for CLI argument parsing.
- `serde` and `serde_json` for config, provider payloads, tool arguments, and JSONL conversation records.
- `reqwest` with streaming support for OpenAI-compatible API requests.
- `schemars` or handwritten JSON schemas for tool definitions.
- `regex`, `walkdir`, and optionally `ignore` for the `grep` tool, recursive search, `.gitignore` handling, and binary-file skipping.
- `anyhow` for application-level errors and `thiserror` for structured module errors.
- `chrono` or `time` for timestamps.
- `uuid`, `rand`, or `nanoid` for the random suffix in chat ids.
- `dirs` only for resolving the home directory; the storage path remains fixed at `~/.cass/`.
- `similar` can be considered later for nicer edit previews, but exact replacement is enough for v1.

Recommended development libraries:

- `tempfile` for filesystem tool tests.
- `insta` for snapshot-testing rendered text or JSON records if useful.
- `wiremock` or a lightweight mock provider for integration tests.

## 5. Configuration and Storage

Cass uses `~/.cass/` as the root directory.

```text
~/.cass/
  config.json
  global.md
  conversations/
    <chat-id>.jsonl
```

`config.json` stores provider configuration and defaults:

```json
{
  "provider": "openai-compatible",
  "model": "accounts/fireworks/models/qwen3p7-plus",
  "base_url": "https://api.fireworks.ai/inference/v1",
  "api_key_env": "FIREWORKS_API_KEY",
  "default_access_mode": "read-only"
}
```

The app should create `~/.cass/` and `~/.cass/conversations/` on first run. It may create an empty or commented `~/.cass/global.md` if missing. It should not create a default API key file. API keys should come from environment variables.

Chat ids are timestamp-based plus a short random suffix, for example `2026-06-20-143012-a8f3`. `--resume <chat-id>` loads `~/.cass/conversations/<chat-id>.jsonl` and rebuilds the conversation context. `--resume` without a chat id lists chats whose stored `cwd` matches the current launch directory or `--cwd`.

## 6. JSONL Conversation Format

Each line is one JSON object. This keeps appends simple and makes corrupted tail recovery easier.

Recommended event records:

```json
{"type":"meta","chat_id":"...","created_at":"...","model":"accounts/fireworks/models/qwen3p7-plus","cwd":"..."}
{"type":"system","content":"..."}
{"type":"message","role":"user","content":"...","created_at":"..."}
{"type":"message","role":"assistant","content":"...","tool_calls":[...],"created_at":"..."}
{"type":"tool_result","tool_call_id":"...","name":"read","content":"...","ok":true,"created_at":"..."}
```

When reconstructing context for the model, convert the JSONL records into provider-specific message objects. The stored data should not include UI-only information such as cursor position, terminal size, collapsed/expanded display state, transient status banners, or keypress history.

The conversation should store the stable base system prompt for that chat, including the contents of `~/.cass/global.md` at chat creation time. Dynamic runtime details such as current access mode, currently allowed tools, and current filesystem boundary should be injected when building the provider request for each turn, so toggling `Shift-Tab` changes the effective instructions immediately. This keeps resumed conversations reproducible while avoiding a stale read-only/full-access prompt after mode changes.

## 7. System Prompt Shape

The system prompt should be hardcoded in `src/prompt.rs` and written as paragraph-separated numbered sections, not as dense bullet lists. It should describe Cassady's role, access-mode constraints, read-only workspace limits, tool-use expectations, streaming expectations, and editing rules.

`~/.cass/global.md` provides user-specific extra instructions. Cass should insert the contents of `global.md` near the top-middle of the system prompt, after the identity/operating-style section and before the detailed access-mode and tool rules. If `global.md` is empty or missing, omit that section cleanly.

Example structure:

```text
1. Identity and operating style

You are Cassady, a minimal coding agent running in a terminal chat interface. Work carefully, explain concise next steps, and prefer inspecting files before changing them.

2. User global instructions

The following additional instructions were provided by the user and should be followed when they do not conflict with runtime safety constraints:

<contents of ~/.cass/global.md>

3. Access modes and filesystem boundaries

You are operating under an access mode supplied by the runtime. In read-only mode you may inspect files with ls, read, and grep only inside the launch working directory. In full-access mode you may request write and edit when needed and paths are not restricted to the launch directory by Cass.

4. Reading and searching files

When reading context, batch related reads into a single read tool call. The read tool accepts multiple files and optional line ranges per file, so prefer one well-scoped call over many small calls. When a file or directory may be too large to read directly, use grep to locate relevant lines before reading narrower ranges.

5. Editing files

Use edit for targeted changes. Each edit must identify exact old text that appears uniquely in the file and replacement text. Do not use write to make small modifications to existing files unless a full rewrite is explicitly intended.
```

The actual prompt builder should receive the stored base prompt/global instructions plus current access mode, launch working directory, current unrestricted/full-access status, model name, and allowed tool list. It should rebuild the effective provider system prompt on every turn.

## 8. Access Modes and Path Boundaries

Define an `AccessMode` enum:

```rust
enum AccessMode {
    ReadOnly,
    FullAccess,
}
```

Mode behavior:

- `ReadOnly` permits `ls`, `read`, and `grep` only.
- `ReadOnly` restricts all tool paths to the launch working directory or the path supplied by `--cwd`.
- `FullAccess` permits `ls`, `read`, `grep`, `write`, and `edit`.
- `FullAccess` does not restrict paths to the launch working directory, though tools still operate under the user's OS permissions.

Tool enforcement must happen in code, not only in the system prompt. If a model requests a disallowed tool, Cass returns a tool result explaining that the tool is unavailable in the current mode and asks the model to continue within constraints.

Path enforcement should be centralized in `tools/path.rs`. In read-only mode, resolve symlinks/canonical paths where possible and reject paths escaping the allowed root. For paths that do not exist, read-only tools should generally fail because they only inspect existing files/directories. In full-access mode, expand `~` and resolve relative paths against the current working directory without applying the read-only root restriction.

## 9. Tool Interfaces

Represent each tool with:

- name
- description
- JSON schema for arguments
- Rust executor function
- access mode requirement

The provider adapter converts these definitions into OpenAI-compatible tool schema format.

### `ls`

Arguments:

```json
{"path":"."}
```

Behavior:

- Expands `~`.
- Resolves relative paths against the current working directory.
- In read-only mode, rejects paths outside the launch root.
- Returns a compact directory listing with directories clearly marked.
- Does not recursively list by default.
- Fails cleanly if the path does not exist or is not a directory.

### `read`

Arguments should support multiple files in one call:

```json
{
  "files": [
    {"path":"README.md"},
    {"path":"src/app.rs","lines":"35-60"}
  ]
}
```

Behavior:

- Expands `~` and resolves relative paths against the current working directory.
- In read-only mode, rejects paths outside the launch root.
- Allows optional 1-indexed inclusive line ranges like `35-60`, `35-`, or `-60`.
- Returns each file with a header containing path and selected line range.
- Adds line numbers in output.
- Applies output limits per file and overall to avoid oversized context.
- Treats binary files as unsupported unless later image or binary support is explicitly added.

### `grep`

Arguments should support searching a single file, multiple files, or directories:

```json
{
  "query": "function main",
  "paths": ["src", "README.md"],
  "regex": false,
  "case_sensitive": false,
  "context_lines": 2,
  "max_matches": 100
}
```

Behavior:

- Expands `~` and resolves relative paths against the current working directory.
- In read-only mode, rejects paths outside the launch root.
- Searches files directly and walks directories recursively.
- Skips common heavy directories such as `.git`, `target`, `node_modules`, and vendor/cache folders by default.
- Respects `.gitignore` when using the `ignore` crate, unless a later option disables that behavior.
- Supports literal search by default and optional regex search.
- Returns matching path, line number, matching line, and optional surrounding context lines.
- Applies match-count and output-size limits so the model can search large projects without reading whole files into context.
- Treats binary files as unsupported/skipped.

### `write`

Arguments:

```json
{
  "path":"path/to/file.txt",
  "content":"new full file contents"
}
```

Behavior:

- Requires full-access mode.
- Expands `~` and resolves relative paths against the current working directory.
- Creates parent directories as needed.
- Overwrites existing files or creates new files.
- Writes atomically where practical by writing to a temporary file in the destination directory and renaming it into place. Atomic writing means Cass avoids leaving a half-written destination file if the process crashes or the disk write fails midway.
- Returns a concise success result with bytes written.

### `edit`

Arguments:

```json
{
  "path":"path/to/file.rs",
  "edits":[
    {"old_text":"exact existing text","new_text":"replacement text"}
  ]
}
```

Behavior:

- Requires full-access mode.
- Reads the original file once.
- Every `old_text` must match exactly one unique non-overlapping region in the original file.
- All edits are validated against the original content before writing anything.
- If any edit fails because text is missing, duplicated, or overlapping, no changes are written.
- If validation passes, apply all replacements and write the file atomically where practical, using the same temporary-file-and-rename approach as `write`.
- Return a concise result with number of edits applied.

This mirrors the safe exact-replacement behavior used by coding-agent edit tools.

## 10. Agent Loop

The agent loop should follow this flow:

1. Load or create the conversation.
2. Build the effective system prompt from the stored base prompt plus current access mode, launch working directory, allowed tools, and model name.
3. Append the user's message to JSONL.
4. Send context, system prompt, and allowed tools to the provider.
5. Stream assistant text blocks line by line to the UI and append the final assistant message when complete.
6. If the assistant emits tool calls, stream the tool call name and arguments to the UI, append the assistant tool-call message, execute each tool, stream each tool result under its call, append each result, and call the model again.
7. Continue until a final assistant message is produced.

Do not impose a fixed max tool-call or tool-iteration limit per user turn. If runaway behavior becomes a concern, address it with user cancellation or provider-level controls rather than an arbitrary per-turn cap.

The agent should emit internal UI events rather than writing directly to the terminal. Example events:

```rust
enum AgentEvent {
    AssistantLine(String),
    ToolCallStarted { id: String, name: String, arguments: serde_json::Value },
    ToolResult { id: String, name: String, ok: bool, content: String },
    Status(String),
    TurnFinished,
}
```

This lets the UI render streaming output, truncate or expand tool results with `Ctrl-O`, and keep the agent loop testable.

### Context and tool-result limits

For v1, Cass should keep context management simple. Reconstruct the conversation from JSONL and send recent context that fits within configured limits. If the conversation becomes too large, prefer dropping oldest non-system messages from the provider request and keep the full JSONL on disk. If the provider still rejects the request as too large, show a clear `context too large` error and keep the chat open.

Tool results have two separate limits. The model-facing result should be large enough to be useful but capped to avoid exhausting context; large reads should recommend using `grep` or narrower line ranges. The UI-facing result is truncated more aggressively by default and can be expanded with `Ctrl-O`. Display truncation must not change what is stored in JSONL or what is sent back to the model.

## 11. Provider Abstraction

Start with one provider implementation: a general OpenAI-compatible chat completions client. The default configuration points at Fireworks, but the code should work with other OpenAI-compatible endpoints when `base_url`, model, and API key environment variable are configured.

Provider interface:

```rust
#[async_trait::async_trait]
trait ChatProvider {
    async fn stream_complete(
        &self,
        messages: Vec<ModelMessage>,
        tools: Vec<ToolSpec>,
    ) -> Result<ProviderStream, ProviderError>;
}
```

The OpenAI-compatible implementation reads:

- model from CLI args, config, or default, defaulting to `accounts/fireworks/models/qwen3p7-plus`
- base URL from CLI args, config, or default, defaulting to `https://api.fireworks.ai/inference/v1`
- API key environment variable from CLI args, config, or default, defaulting to `FIREWORKS_API_KEY`

Configuration precedence is: CLI args first, then `~/.cass/config.json`, then built-in defaults. Normalize `base_url` so users can provide either the API root or the full chat-completions URL without accidentally producing a doubled path.

Avoid tying the rest of the app to raw provider response shapes. Convert streaming deltas and final responses into internal assistant text, tool-call, and completion events. Because OpenAI-compatible endpoints vary, support streamed assistant text as the primary path and handle tool calls whether they arrive as streamed deltas or only in a final assistant message.

## 12. CLI Arguments

Initial CLI:

```text
cass [--resume [CHAT_ID]] [--model MODEL] [--base-url URL] [--api-key-env ENV] [--cwd PATH]
cassady [--resume [CHAT_ID]] [--model MODEL] [--base-url URL] [--api-key-env ENV] [--cwd PATH]
```

Behavior:

- `--resume CHAT_ID`: loads an existing conversation JSONL file.
- `--resume` without a chat id: lists past chats whose stored `cwd` matches the current `--cwd`/launch directory, showing chat id, creation time, model, and first user-message preview, then exits.
- `--model MODEL`: overrides configured model for the session.
- `--base-url URL`: overrides configured provider base URL.
- `--api-key-env ENV`: overrides configured API key environment variable for the session.
- `--cwd PATH`: sets the launch root and working directory for relative tool paths. Default is the shell's current working directory.

Later optional flags:

- `--config PATH`
- `--readonly`
- `--full-access`
- `--list-chats`
- `--unsafe-global-readonly` if a future mode is needed to disable read-only root restriction.

Do not add slash commands for v1. Keep v1 key-driven plus CLI flags.

## 13. UI Implementation

Use `ratatui` and `crossterm` first. This gives enough control to create a polished terminal experience like Pi without requiring a complicated application architecture.

Suggested layout:

- Header/status bar: app name, chat id, model, access mode, cwd.
- Transcript area: user messages, streamed assistant lines, tool calls, and tool results.
- Input area: multiline user message editor.
- Footer/help bar: `Shift-Tab mode`, `Ctrl-J newline`, `Ctrl-O tool output`, `Ctrl-C clear/exit`.

Key handling:

- `Shift-Tab`: toggles access mode only when the agent is idle.
- `Ctrl-J`: inserts a newline into the user input.
- `Ctrl-O`: toggles tool output display between truncated and full.
- `Ctrl-C`: first press clears input and arms exit; second press within 1.5 seconds exits and prints `Resume this chat with: cass --resume <id>` after terminal cleanup.
- `Enter`: sends the message if the input is non-empty.

Streaming display:

- Assistant message blocks stream line by line.
- Tool calls render as visible blocks with tool name and formatted JSON arguments.
- Tool results render directly beneath their tool call.
- Tool results are truncated by default with an indicator such as `… truncated, press Ctrl-O to expand`.
- The `Ctrl-O` state is UI-only and must not be persisted in conversation JSONL.

Transcript navigation should optimize for a clean, user-friendly experience. Auto-scroll while new output streams if the user is already at the bottom; preserve the user's scroll position if they have scrolled up. Support mouse wheel and `PageUp`/`PageDown` for transcript scrollback, handle terminal resize without corrupting layout, and keep the input box visible.

## 14. Error Handling

Tool errors should be returned to the model as normal tool results with `ok: false`, not crash the app. Provider errors should show a concise user-facing message and keep the conversation open. Conversation append failures should be treated as serious and displayed clearly because persistence is core behavior.

For malformed JSONL during resume, load valid records until the first corrupted line and warn the user. Do not silently drop data.

If the configured API key environment variable is missing, show a clear setup error indicating which variable must be set, while still allowing the user to exit cleanly. With default settings, this variable is `FIREWORKS_API_KEY`.

## 15. Testing Plan

Unit tests:

- Conversation JSONL append and reload.
- Resume with missing chat id.
- `--resume` without a chat id lists chats for the current cwd.
- Resume with partially corrupted JSONL.
- Effective system prompt includes hardcoded prompt and inserts `global.md` near the top-middle.
- Access-mode tool filtering.
- Disallowed tool-call handling.
- Read-only path restriction rejects paths outside `--cwd`, including symlink escapes.
- Full-access path resolution does not apply the read-only root restriction.
- `ls` path resolution and error behavior.
- `read` with full file, bounded ranges, open-ended ranges, multiple files, missing file, binary file, and path outside read-only root.
- `grep` with literal search, regex search, case sensitivity, directory recursion, max match limits, skipped binary files, ignored heavy directories, and path outside read-only root.
- `write` creates parent directories, overwrites content, uses atomic temp-file behavior where practical, and is unavailable in read-only mode.
- `edit` applies single edit, multiple disjoint edits, rejects missing text, rejects duplicate old text, rejects overlapping edits, preserves file when validation fails, and is unavailable in read-only mode.

Integration tests:

- Mock provider streams a final assistant message.
- Mock provider requests `read`, receives streamed tool output, then returns final answer.
- Mock provider requests `grep`, receives search matches, then requests a narrower `read`.
- Mock provider requests disallowed `write` in read-only mode.
- Mock provider requests a path outside the read-only root.
- Resume conversation and continue with previous context.

Manual tests:

- Start a new chat with `cass` and `cassady`.
- Press `Shift-Tab` while idle and confirm mode toggles.
- Press `Shift-Tab` while the agent is running and confirm mode does not change.
- Press `Ctrl-J` and confirm it inserts a newline rather than sending.
- Press `Ctrl-O` and confirm tool results expand/collapse.
- Press `Ctrl-C` once and confirm input clears.
- Press `Ctrl-C` twice within 1.5 seconds and confirm exit.
- Confirm exit prints `Resume this chat with: cass --resume <id>`.
- Verify assistant output streams line by line.
- Verify tool calls and truncated tool results display live.
- Create a file in full-access mode using `write`.
- Modify a file in full-access mode using `edit`.

## 16. Implementation Milestones

### Milestone 1: Rust skeleton and persistence

Set up `Cargo.toml`, module layout, CLI argument parsing, config loading, `~/.cass/` initialization, `global.md` loading, chat id generation, JSONL append, and resume loading.

### Milestone 2: Tools and access control

Implement `ls`, `read`, `grep`, `write`, and `edit` with strong validation and tests. Add access-mode filtering, read-only root restriction, full-access unrestricted behavior, and disallowed-tool handling.

### Milestone 3: Provider and agent loop

Implement the provider abstraction, general OpenAI-compatible streaming provider with Fireworks defaults, message conversion, unbounded tool-call loop, context limiting, and system prompt generation.

### Milestone 4: Terminal UI

Implement the `ratatui`/`crossterm` interface, transcript rendering, input box, `Shift-Tab` mode toggle while idle only, `Ctrl-J` newline input, `Ctrl-O` tool output expansion, streaming assistant lines, streaming tool calls/results, scrollback behavior, resize handling, and double-`Ctrl-C` exit behavior.

### Milestone 5: Polish and documentation

Add README usage instructions, configuration examples, OpenAI-compatible endpoint setup, Fireworks API key setup, test coverage, linting, formatting, and packaging so users can install both `cass` and `cassady` commands with `cargo install --path .`.

## 17. Resolved Product Decisions

- The implementation language is Rust, not Python.
- The installed command should be both `cass` and `cassady`.
- The provider implementation should support general OpenAI-compatible endpoints.
- The default configuration points at Fireworks.
- The default model is `accounts/fireworks/models/qwen3p7-plus`.
- The default API key environment variable is `FIREWORKS_API_KEY`.
- Read-only mode restricts paths to the launch working directory or `--cwd`.
- Full-access mode does not apply the read-only path restriction.
- `Shift-Tab` only toggles mode while the agent is idle.
- `Ctrl-J` inserts a newline and `Enter` sends.
- Chat ids use timestamp plus random suffix.
- `--resume` without a chat id lists past chats for the current directory.
- The system prompt is hardcoded and can be augmented by `~/.cass/global.md` inserted near the top-middle.
- Assistant message blocks stream line by line.
- Tool calls and tool results are displayed live.
- Tool results are truncated by default and can be expanded/collapsed with `Ctrl-O`.
- v1 has no shell/bash tool.
- v1 has no slash commands.
- v1 uses atomic writes where practical for `write` and `edit`, by writing to a temporary file first and renaming it into place after validation/write success.
- v1 installs with `cargo install --path .`.

## 18. Remaining Open Decisions

- Exact numeric truncation limits for displayed tool results and model-facing tool results.
- Exact context-window budget defaults for OpenAI-compatible providers that may not report limits consistently.
- Exact default `grep` ignore rules and maximum recursive search size.
