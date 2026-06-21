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
  "default_access_mode": "read-only"
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

- Type `/`: show command autocomplete
- `Up`/`Down`: move through an autocomplete menu
- `Tab`/`Enter`: fill autocomplete selection
- `Enter`: send message / run command when no autocomplete menu is open
- `Ctrl-J`: insert newline
- `Shift-Tab`: toggle read-only/full-access mode while idle
- `Ctrl-O`: toggle truncated/full tool output display
- `PageUp`/`PageDown` or mouse wheel: transcript scroll
- `Ctrl-C` twice within 1.5 seconds: exit

## Commands

- `cass check`: validate Cass config files
- `/model <model>`: switch the model for future turns; model autocomplete lists entries from `~/.cass/models.json`
- `/resume <chat>`: resume a saved chat; chat autocomplete lists chats for the current directory
- `/status`: show current chat status

On exit Cass prints:

```text
Resume this chat with: cass --resume <id>
```

## Tools

Read-only mode allows `ls`, `read`, and `grep` within the launch cwd/`--cwd` and the bundled docs directory at `~/.cass/docs`.

Full-access mode additionally allows `write` and `edit`. Mutating tools use atomic writes where practical: Cass writes to a temporary file first, then renames it into place after validation/write success. `write` and `edit` are always blocked under `~/.cass/docs`.

There is no shell/bash tool in v1.
