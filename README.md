# Cassady / Cass

Cassady (`cass`) is a terminal coding agent written in Rust. It runs an interactive chat in your project, can inspect files, apply exact edits, run shell commands when the active safety mode allows them, and persist sessions for later resume. Cassady talks to OpenAI-compatible providers and a built-in ChatGPT Codex provider preset.

The project installs two equivalent commands, `cass` and `cassady`; examples use `cass`.

## Current scope and limitations

- Provider support includes OpenAI-compatible chat/completions APIs plus the `ChatGPT Codex` preset for users already signed in to Codex.
- The primary interface is an interactive terminal UI.
- An experimental Rust embedding API for headless sessions is available (added in v0.2.6); it is useful for early integrations but not yet a stable long-term library contract.
- Config and conversation state live under `~/.cass`.
- Windows binaries are built for releases, but deeper Windows terminal, path, shell, and filesystem polish is planned for a later release.
- `cass update` can update release-archive installs from official GitHub releases; external package managers should still be updated through their own tools.

## Install from source

```sh
cargo install --path .
```

This installs both commands:

```sh
cass --version
cassady --version
```

## First use

Start Cassady in a project directory:

```sh
cass
```

If Cassady cannot resolve a usable provider, model, or API key, it offers to run setup before opening a chat. You can also run setup or provider login explicitly:

```sh
cass login
cass setup
cass check
cass update --check
cass
```

The setup wizard lets you choose one or more providers. OpenAI-compatible providers use an API key environment variable and can discover models from `GET /models` when the key is available. `ChatGPT Codex` uses your existing local Codex login (`~/.codex/auth.json`) and the Codex responses endpoint instead of a Cassady API-key environment variable.

Set your provider key in the shell where you run Cassady. For example, on macOS/Linux:

```sh
export OPENAI_API_KEY=...
```

In PowerShell:

```powershell
$env:OPENAI_API_KEY = "..."
```

Run `cass check` any time to validate JSON config, provider/model references, active model resolution, and API key availability.

## Everyday usage

```sh
cass [--model MODEL] [--cwd PATH]
cass --resume <chat-id>
cass --resume
cass check
cass login
cass logout
cass setup
cass update
```

`cass --resume` without an id lists saved chats for the current directory. `cass update` checks official GitHub releases and can update both `cass` and `cassady` in the current install directory. When Cassady exits a chat, it prints a resume command for that session.

Common in-chat commands:

- `/branch` or `/restore`: open the branch/restore menu.
- `/login`: configure or update provider login settings.
- `/logout`: remove saved provider config and associated model entries.
- `/fast`, `/fast on`, `/fast off`, `/fast status`: prefer faster inference when the active provider/model supports it. ChatGPT Codex models, including `gpt-5.5`, are treated as fast-capable.
- `/model <model>`: switch to a model from `~/.cass/models.json`.
- `/new`: create a new chat for the current directory.
- `/resume <chat>`: resume a saved chat for the current directory.
- `/status`: show chat id, model, fast-mode state, mode, cwd, record count, and current status.

Helpful keys:

- `/`: show command autocomplete.
- `Enter`: send the message or accept an autocomplete item.
- `Ctrl-J` or `Ctrl-Enter`: insert a newline.
- `Shift-Tab`: cycle access mode while idle.
- `Tab`: cycle reasoning effort while idle.
- `Ctrl-O`: toggle compact/full tool output display.
- `Ctrl-Shift-R` or `Ctrl-R`: toggle reasoning display.
- `Esc`: request turn cancellation while a turn is running; while idle, press twice within 1.5 seconds to open branch/restore.
- `Ctrl-C` twice within 1.5 seconds: exit.

## Safety model

Cassady exposes tools according to the active access mode:

- `read-only`: read/list/search the workspace and bundled docs. No edits or shell commands.
- `workspace-edit`: read/list/search plus write/edit inside the launch workspace. Shell commands require explicit approval.
- `full-access`: read/write/edit broadly under your OS permissions and run shell commands without the workspace-edit approval prompt. Bundled docs remain read-only.

Use `--readonly`, `--workspace-edit`, or `--full-access` to choose a mode at launch, or press `Shift-Tab` while idle.

## Branch and restore

Press `Esc` twice while idle, or type `/branch`, to browse the current conversation's branch family. Selecting an earlier user message, assistant message, tool call, or tool result creates a new branch conversation instead of truncating the original chat. The menu also lets you switch back to related branches later.

Conversation-only branching is the safe default. If you choose file restore, Cassady restores only file changes it tracked from successful `write` and `edit` tools. Shell commands, manual edits, unsupported files, and hash conflicts are reported or skipped rather than overwritten blindly.

## Configuration and docs

Cassady stores user-editable files in `~/.cass`:

- `config.json`: active defaults and preferences.
- `providers.json`: provider base URLs and API key references.
- `models.json`: model metadata.
- `global.md`: optional global instructions added to new chat system prompts when they fit the active request; they cannot override access modes, tool denials, approvals, or workspace boundaries.
- `docs/`: bundled documentation installed from the current binary.

API key references should usually be written as environment variables such as `"$OPENAI_API_KEY"`. The `ChatGPT Codex` provider is different: it stores no key in `~/.cass` and reads the bearer token from local Codex auth at check/request time.

Detailed bundled docs live in this repository under [`docs/`](docs/README.md) and are installed to `~/.cass/docs` at runtime.

## Experimental Rust embedding API

Rust applications can import Cassady and run headless sessions without launching the TUI:

```rust
use cassady::prelude::*;

let session = SessionBuilder::new()
    .cwd(".")
    .access_mode(AccessMode::ReadOnly)
    .build()
    .await?;
```

See [Experimental Rust embedding API](docs/embedding.md) for session creation, streamed events, approval handling, cancellation, and current limitations.

## More documentation

- [Commands](docs/commands.md)
- [Configuration](docs/configuration.md)
- [Providers and models](docs/providers.md)
- [Access modes and tool safety](docs/access-modes.md)
- [Experimental Rust embedding API](docs/embedding.md)
- [Workflows](docs/workflows.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Platform notes](docs/platforms.md)
- [Glossary](docs/glossary.md)
