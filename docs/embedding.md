# Experimental Rust embedding API

Cassady v0.2.6 includes an experimental Rust API for running headless agent sessions from another Rust program. The API is intended for early integrations and may change before Cassady declares a stable library contract.

The embedding API uses the same provider configuration, global instructions, prompts, access modes, tools, and JSONL conversation storage as the `cass` terminal UI. By default it reads and writes under `~/.cass`, so run `cass setup` first or create compatible `config.json`, `providers.json`, and `models.json` files programmatically. The v0.4.0 desktop app in `cassady-desktop/` is a reference embedding built on this API.

## Minimal example

```rust
use cassady::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let session = SessionBuilder::new()
        .cwd(std::env::current_dir()?)
        .access_mode(AccessMode::ReadOnly)
        .build()
        .await?;

    let mut turn = session
        .start_turn("Summarize this project in a few sentences.")
        .await?;

    while let Some(event) = turn.next_event().await? {
        match event {
            Event::AssistantChunk(text) => print!("{text}"),
            Event::Finished => break,
            _ => {}
        }
    }

    let session = turn.finish().await?;
    eprintln!("\nResume chat with: cass --resume {}", session.id());
    Ok(())
}
```

Add Cassady from a git checkout or path dependency, and ensure your application runs on Tokio.

## Creating or resuming sessions

Use `SessionBuilder` to set host-controlled options:

```rust
let session = SessionBuilder::new()
    .config_root("/tmp/my-cass-root")
    .cwd("/path/to/workspace")
    .access_mode(AccessMode::WorkspaceEdit)
    .model("my-model")
    .base_url("https://provider.example/v1")
    .api_key_env("MY_PROVIDER_KEY")
    .build()
    .await?;

let resumed = SessionBuilder::new()
    .cwd("/path/to/workspace")
    .resume(session.id())
    .await?;
```

`build()` is equivalent to `new_session()`. Resumed and new sessions use Cassady's normal `conversations/*.jsonl` files, so CLI and embedded sessions can interoperate.

## Events and approvals

`Session::start_turn` consumes the session and returns a `Turn`. This type design prevents overlapping turns for the same session. Call `turn.finish().await?` after receiving `Event::Finished` to recover the updated `Session`.

Important events include:

- `AssistantChunk` and `ReasoningChunk`
- `ToolCallStarted`, `ToolOutputChunk`, and `ToolResult`
- `ApprovalRequested` and `ApprovalResolved`
- `Status`
- `Finished`

When a tool needs approval, decide in host code:

```rust
while let Some(event) = turn.next_event().await? {
    match event {
        Event::ApprovalRequested(request) => {
            eprintln!("approval needed for {}: {}", request.name, request.reason);
            turn.deny(&request.request_id)?;
        }
        Event::Finished => break,
        _ => {}
    }
}
```

The approval policy is the same as the TUI: shell is unavailable in `read-only`, requires approval in `workspace-edit`, and runs directly in `full-access` unless destructive-operation confirmation is enabled.

## Cancellation

Dropping a `Turn` aborts the underlying task. Prefer `turn.cancel().await?` when you want Cassady to repair the conversation with cancellation records before returning the session.

## Current limitations

The v0.2.6 API is intentionally small and experimental. It does not include custom provider traits, custom tools, plugin loading, multi-agent orchestration, background daemons, task queues, or a synchronous/blocking wrapper.
