# v0.2.6 Rust Embedding API Implementation Plan

## Goal

v0.2.6 adds the first intentional public Rust API for embedding Cassady in another Rust project. A developer should be able to add Cassady as a dependency, configure a workspace/model/access mode, start a headless agent session, send user messages, receive streamed agent events, and handle approval requests without launching the interactive TUI.

Success statement:

> A small Rust program can import `cassady`, start a new headless session in a workspace, stream assistant/tool events from a turn, optionally approve shell requests, and inspect the updated conversation state using documented experimental APIs.

## Scope

### In scope

- Add an experimental embedding API module with cohesive public types instead of requiring callers to wire together internal modules directly.
- Support starting a new headless agent session from Rust code.
- Support resuming an existing conversation by id when using Cassady's existing conversation storage.
- Support running one turn at a time and streaming typed events to the host application.
- Expose approval handling for tools that require host/user consent, especially shell in `workspace-edit` mode.
- Reuse the existing config, provider, prompt, security, conversation, and tool execution paths used by the CLI/TUI.
- Provide simple builder/options types for cwd, access mode, model/base URL/API key overrides, reasoning effort, and Cassady config root.
- Add a crate-level `prelude` or clearly documented imports for common embedding use.
- Add docs and examples that show a minimal headless integration.
- Add integration tests that exercise the public API without a terminal.

### Out of scope

- Declaring the Rust API stable for semver compatibility. The API should be explicitly marked experimental in v0.2.6.
- Replacing the CLI/TUI as the primary user interface.
- Multi-agent orchestration, task queues, background daemons, schedulers, or distributed workers.
- Custom model provider traits or non-OpenAI-compatible protocols.
- User-defined custom tools or plugin loading.
- A synchronous/blocking API. The first embedding surface can require Tokio.
- Exposing low-level terminal UI internals as supported public API.
- Publishing to crates.io as part of this release unless separately requested.

## Context and Current State

Cassady already builds a library crate:

- `Cargo.toml` defines `[lib] name = "cassady" path = "src/lib.rs"`.
- `src/lib.rs` currently re-exports many internal modules directly and exposes `run()` for the CLI/TUI path.
- `src/agent.rs` contains the core async turn loop:
  - `AgentSettings`
  - `AgentEvent`
  - `AgentCommand`
  - `run_turn(...)`
  - `run_turn_with_commands(...)`
- `src/app.rs` owns interactive startup, TUI state, chat creation/resume, cancellation, approval UI, and local slash commands.
- `src/conversation.rs` persists conversations as JSONL and can create/load/list chats.
- `src/config.rs` loads providers, models, active defaults, API key references, access mode, tool limits, and docs paths.
- `src/security.rs` centralizes access-mode decisions.
- `src/tools/*` implements the same tools that headless sessions should use.

The current crate can technically be imported, but the supported path is unclear: callers must know which internal modules to combine, how to create base prompts, how to load config safely, how to route approval commands, and how to consume events. v0.2.6 should add a thin, intentional API layer over these internals.

## Design Principles

1. **Thin wrapper over proven internals.** Reuse the same agent loop and policy code as the CLI so embedded behavior matches interactive behavior.
2. **Explicitly experimental.** Make the new API useful without promising final naming or long-term stability yet.
3. **Headless first.** The API should not depend on `ratatui`, terminal setup, crossterm event loops, or slash-command UI state.
4. **Host owns presentation.** Embedded callers receive typed events and decide how to display assistant chunks, tool calls, approvals, and errors.
5. **Safe defaults.** Default to `read-only`, environment-variable API keys, existing Cassady config files, and workspace-rooted paths.
6. **Approval is part of the API.** Hosts must be able to approve or deny requests rather than having Cassady assume a TUI is present.
7. **Keep the first surface small.** Prefer one clear session builder and one turn-running method over exposing every internal knob.

## Design

### Module layout

Add a new module, for example:

```rust
pub mod embedding;
pub mod prelude;
```

`src/embedding.rs` should be the supported experimental API. Existing internal modules can remain public in v0.2.6 for compatibility, but docs should steer new users toward `cassady::embedding` or `cassady::prelude`.

Suggested public surface:

```rust
pub struct SessionBuilder { ... }

pub struct Session { ... }

pub struct SessionOptions { ... }

pub struct Turn { ... }

pub enum Event { ... }

pub enum Command { ... }

pub struct ConversationInfo { ... }
```

The exact names can change during implementation, but they should avoid leaking TUI-specific terms.

### Builder and options

Provide a builder that covers common embedding setup:

```rust
let mut session = cassady::embedding::SessionBuilder::new()
    .cwd("/path/to/project")
    .access_mode(AccessMode::WorkspaceEdit)
    .model("accounts/fireworks/models/qwen3p7-plus")
    .build()
    .await?;
```

Builder responsibilities:

- Resolve and canonicalize `cwd` like CLI startup.
- Load config from the default Cassady root unless an explicit root/path is supplied.
- Apply model/base URL/API key env overrides without requiring a `Cli` value from callers.
- Resolve API key availability before starting a turn and return a useful error.
- Install or locate bundled docs as needed by `Config::load` behavior.
- Create the base system prompt with `~/.cass/global.md` when starting a new conversation.
- Default access mode to config/default, then builder override, then `read-only` if no config exists.

Avoid requiring callers to import or construct `cli::Cli`.

### New and resumed sessions

Support at least:

```rust
let session = SessionBuilder::new().cwd(".").new_session().await?;
let session = SessionBuilder::new().cwd(".").resume("chat-id").await?;
```

A `Session` should expose lightweight metadata:

```rust
session.id();
session.cwd();
session.model();
session.access_mode();
session.conversation_path();
```

The conversation should continue to be persisted in the same JSONL format so CLI and library sessions can interoperate.

### Running a turn

Provide a headless one-turn API that streams events:

```rust
let mut turn = session.start_turn("Explain the crate layout").await?;
while let Some(event) = turn.next_event().await? {
    match event {
        Event::AssistantChunk(text) => print!("{text}"),
        Event::ApprovalRequested(request) => {
            turn.approve(request.id).await?;
        }
        Event::Finished => break,
        _ => {}
    }
}
let session = turn.finish().await?;
```

Alternative designs are acceptable, such as returning `(EventStream, CommandSink)` plus a completion handle, as long as examples are simple and approval commands are supported.

The wrapper can map `agent::AgentEvent` and `agent::AgentCommand` into public embedding types. It should avoid exposing internal channel mechanics unless that is the cleanest Tokio-native API.

### Event model

Expose typed events that are stable enough for hosts to build UI/logging around:

- assistant text chunks
- reasoning chunks, when provider/model returns them
- tool call started
- tool output chunk
- tool result
- approval requested
- approval resolved
- status
- turn finished
- error or turn failure

The public event type can wrap or re-export `agent::AgentEvent` initially, but the plan should prefer a dedicated type if it prevents low-level internals from becoming accidental API.

### Approval behavior

Approval requests should include:

- request id
- tool call id
- tool name
- arguments
- human-readable reason

The host should be able to approve or deny by request id. If the host drops the turn or never responds, cancellation/drop behavior should be documented.

For v0.2.6, keep approval policy aligned with `security.rs`:

- `read-only`: shell unavailable.
- `workspace-edit`: shell asks.
- `full-access`: shell allowed.

### Cancellation and drop behavior

The TUI already cancels by aborting the agent task and repairing pending records. The embedding API should define a basic behavior:

- Dropping an active turn should abort the underlying task if possible.
- A simple explicit `cancel()` method is preferred if practical.
- Conversation repair for cancelled turns can be minimal in v0.2.6, but pending tool calls must not corrupt resumed conversations.

If full parity with the TUI cancellation path is too large, document the limitation and add tests for the supported behavior.

### Error handling

Use a public result alias such as:

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

The first pass may wrap `anyhow::Error`, but public errors should include enough context for embedding callers to distinguish:

- config load errors
- missing API key
- provider request errors
- conversation load/create errors
- active turn already running
- approval request not found or already resolved

Do not panic for ordinary configuration or runtime failures.

### Examples

Add at least one compilable example under `examples/`, for example `examples/headless_agent.rs`:

```rust
use cassady::prelude::*;

#[tokio::main]
async fn main() -> cassady::embedding::Result<()> {
    let mut session = SessionBuilder::new()
        .cwd(std::env::current_dir()?)
        .access_mode(AccessMode::ReadOnly)
        .build()
        .await?;

    let mut turn = session.start_turn("Summarize this project.").await?;
    while let Some(event) = turn.next_event().await? {
        if let Event::AssistantChunk(text) = event {
            print!("{text}");
        }
    }
    turn.finish().await?;
    Ok(())
}
```

The example should be honest about requiring configured providers and API keys.

## Implementation Steps

1. **Define the experimental API shape.** Add `src/embedding.rs` with builder, session, turn, event, command/approval, and result/error types.
2. **Add non-CLI config loading helpers.** Refactor or add helpers in `src/config.rs` so library callers can apply overrides without constructing `cli::Cli`.
3. **Extract chat creation/resume helpers.** Move reusable prompt/global/conversation setup out of `src/app.rs` into functions usable by both TUI and embedding API.
4. **Wrap the existing agent loop.** Use `agent::run_turn_with_commands` internally and provide a host-friendly event stream plus approval methods.
5. **Handle turn lifecycle.** Ensure a session cannot run overlapping turns unless explicitly supported; persist and return the updated conversation after a turn finishes.
6. **Add cancellation/drop handling.** Provide at least a documented `cancel()` path and avoid leaving pending tool-call records in a corrupted state.
7. **Add examples and docs.** Create a headless example and a bundled docs page for the experimental Rust API.
8. **Update README and crate exports.** Add `embedding`/`prelude` exports and a short README section pointing to the new docs.
9. **Test the public surface.** Add integration tests with a mock OpenAI-compatible server and temporary config/conversation roots.

## Tests

- Unit tests for builder option precedence: default config, explicit cwd, access mode, model, base URL, API key env, and config root.
- Integration test that starts a new session and runs a turn against `wiremock`, asserting assistant chunks and persisted conversation records.
- Integration test that resumes an existing conversation through the embedding API.
- Integration test for approval flow in `workspace-edit` mode using a mock tool call that requests shell approval.
- Test that read-only sessions do not expose write/edit/shell tools through the embedded turn.
- Test that starting a second turn while one is active returns an error or is impossible by type design.
- Example compilation through `cargo test --examples` or equivalent.

## Documentation

- Add `docs/rust-api.md` or `docs/embedding.md` describing the experimental API, setup requirements, minimal example, event loop, approval handling, and limitations.
- Link the new page from `docs/README.md` and the README.
- Document that the API is experimental in v0.2.6 and may change before a stable 1.0-style library contract.
- Include a note that embedded sessions use the same `~/.cass` config and conversation storage by default.
- Mention how hosts should run `cass setup` or provide config programmatically before using the API.

## Acceptance Criteria

- A Rust binary in `examples/` can import `cassady`, create a headless session, run a turn, and stream assistant output without launching the TUI.
- Embedded sessions use the same provider, prompt, security, tool, and conversation paths as the CLI.
- Approval requests can be approved or denied programmatically.
- New public API docs and README links clearly label the surface experimental.
- CLI/TUI behavior remains unchanged.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
