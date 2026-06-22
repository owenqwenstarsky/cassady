# Cassady / Cass

Cassady (`cass`) is a minimal Rust terminal coding agent with a looped chat UI, filesystem tools, access modes, JSONL conversation persistence, and OpenAI-compatible LLM support. The default endpoint is Fireworks.

## Install

```sh
cargo install --path .
```

This installs both commands:

```sh
cass
cassady
```

## Configure

By default Cass creates `~/.cass/providers.json` and `~/.cass/models.json` with Fireworks configured:

- base URL: `https://api.fireworks.ai/inference/v1`
- model: `accounts/fireworks/models/qwen3p7-plus`
- API key: `"$FIREWORKS_API_KEY"`

Set your key:

```sh
export FIREWORKS_API_KEY=...
```

User preferences live at `~/.cass/config.json`:

```json
{
  "default_model": "accounts/fireworks/models/qwen3p7-plus",
  "default_access_mode": "read-only",
  "show_reasoning": false
}
```

Provider connection details belong in `~/.cass/providers.json`. Model metadata belongs in `~/.cass/models.json`. API keys may be literal strings or environment-variable references like `"$FIREWORKS_API_KEY"`.

Validate config with:

```sh
cass check
```

Extra global instructions can be placed in `~/.cass/global.md`.

Bundled documentation from this build is embedded into the binary and installed to `~/.cass/docs` on startup. See `~/.cass/docs/configuration.md` for full configuration docs.

## Usage

```sh
cass [--model MODEL] [--base-url URL] [--api-key-env ENV] [--cwd PATH]
cass --resume <chat-id>
cass --resume
cass check
```

`cass --resume` without an ID lists chats for the current directory.

## Keys

- Type `/`: show command autocomplete, including command arguments like `/model <model>` and `/new`
- `Up`/`Down`: move through an autocomplete menu
- `Enter`: fill autocomplete selection when a menu is open; otherwise send message / run command
- `Tab`: cycle reasoning effort (`off` → `low` → `medium` → `high`; required-reasoning models skip `off`)
- `Ctrl-J`: insert newline
- `Shift-Tab`: toggle read-only/full-access mode while idle
- `Ctrl-O`: toggle compact/full tool output display
- `Ctrl-Shift-R`: toggle reasoning display
- `Up`/`Down` or mouse wheel: scroll transcript when no autocomplete menu is open
- `PageUp`/`PageDown`: transcript scroll
- `Ctrl-C` twice within 1.5 seconds: exit

## Commands

- `cass check`: validate Cass config files
- `/model <model>`: switch the model for future turns; model autocomplete lists entries from `~/.cass/models.json`
- `/new`: create a new chat for the current directory
- `/resume <chat>`: resume a saved chat; chat autocomplete lists chats for the current directory
- `/status`: show current chat status

On exit Cass prints:

```text
Resume this chat with: cass --resume <id>
```

## Tools

Tool calls are shown compactly by default; press `Ctrl-O` to expand full tool output.

Reasoning is hidden by default unless `show_reasoning` is enabled; press `Ctrl-Shift-R` to toggle it. Press `Tab` to choose the reasoning effort for future turns. Model metadata controls whether reasoning is supported or required and how the effort is sent to the provider. When providers stream reasoning fields, Cass persists that reasoning and sends it back in future model context using the provider's reasoning field, such as `reasoning_content` or `reasoning`.

Read-only mode allows `ls`, `read`, and `grep` within the launch cwd/`--cwd` and the bundled docs directory at `~/.cass/docs`.

Full-access mode additionally allows `write`, `edit`, and `shell`. Mutating tools use atomic writes where practical: Cass writes to a temporary file first, then renames it into place after validation/write success. `write` and `edit` are always blocked under `~/.cass/docs`. The `shell` tool runs commands via `sh -c` in the launch working directory with a configurable timeout (default 30 seconds) and streams stdout/stderr into the transcript while the command is running.

The `shell` tool is available in full-access mode and runs shell commands in the launch working directory.
