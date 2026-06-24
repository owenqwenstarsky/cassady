# Cassady / Cass

Cassady (`cass`) is a terminal coding agent written in Rust. It runs an interactive chat in your project, can inspect files, apply exact edits, run shell commands when the active safety mode allows them, and persist sessions for later resume. Cassady currently talks to OpenAI-compatible providers.

The project installs two equivalent commands, `cass` and `cassady`; examples use `cass`.

## Current scope and limitations

- Provider support is OpenAI-compatible chat/completions APIs only.
- The primary interface is an interactive terminal UI.
- Config and conversation state live under `~/.cass`.
- Windows binaries are built for releases, but deeper Windows terminal, path, shell, and filesystem polish is planned for a later release.
- Cassady is not an installer, updater, or package manager.

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

If Cassady cannot resolve a usable provider, model, or API key, it offers to run setup before opening a chat. You can also run setup explicitly:

```sh
cass setup
cass check
cass
```

The setup wizard lets you choose one or more OpenAI-compatible providers, enter the API key environment-variable name, discover models from `GET /models` when the key is available, or enter a model id manually.

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
cass setup
```

`cass --resume` without an id lists saved chats for the current directory. When Cassady exits a chat, it prints a resume command for that session.

Common in-chat commands:

- `/model <model>`: switch to a model from `~/.cass/models.json`.
- `/new`: create a new chat for the current directory.
- `/resume <chat>`: resume a saved chat for the current directory.
- `/status`: show chat id, model, mode, cwd, record count, and current status.

Helpful keys:

- `/`: show command autocomplete.
- `Enter`: send the message or accept an autocomplete item.
- `Ctrl-J` or `Ctrl-Enter`: insert a newline.
- `Shift-Tab`: cycle access mode while idle.
- `Tab`: cycle reasoning effort while idle.
- `Ctrl-O`: toggle compact/full tool output display.
- `Ctrl-Shift-R` or `Ctrl-R`: toggle reasoning display.
- `Esc`: request turn cancellation while a turn is running.
- `Ctrl-C` twice within 1.5 seconds: exit.

## Safety model

Cassady exposes tools according to the active access mode:

- `read-only`: read/list/search the workspace and bundled docs. No edits or shell commands.
- `workspace-edit`: read/list/search plus write/edit inside the launch workspace. Shell commands require explicit approval.
- `full-access`: read/write/edit broadly under your OS permissions and run shell commands without the workspace-edit approval prompt. Bundled docs remain read-only.

Use `--readonly`, `--workspace-edit`, or `--full-access` to choose a mode at launch, or press `Shift-Tab` while idle.

## Configuration and docs

Cassady stores user-editable files in `~/.cass`:

- `config.json`: active defaults and preferences.
- `providers.json`: provider base URLs and API key references.
- `models.json`: model metadata.
- `global.md`: optional global instructions added to new chats.
- `docs/`: bundled documentation installed from the current binary.

API key references should usually be written as environment variables such as `"$OPENAI_API_KEY"`.

Detailed bundled docs live in this repository under [`docs/`](docs/README.md) and are installed to `~/.cass/docs` at runtime.

## More documentation

- [Commands](docs/commands.md)
- [Configuration](docs/configuration.md)
- [Providers and models](docs/providers.md)
- [Access modes and tool safety](docs/access-modes.md)
- [Workflows](docs/workflows.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Platform notes](docs/platforms.md)
- [Glossary](docs/glossary.md)
