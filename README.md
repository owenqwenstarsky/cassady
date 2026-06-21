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

By default Cass uses:

- base URL: `https://api.fireworks.ai/inference/v1`
- model: `accounts/fireworks/models/qwen3p7-plus`
- API key env var: `FIREWORKS_API_KEY`

Set your key:

```sh
export FIREWORKS_API_KEY=...
```

Optional config lives at `~/.cass/config.json`:

```json
{
  "provider": "openai-compatible",
  "model": "accounts/fireworks/models/qwen3p7-plus",
  "base_url": "https://api.fireworks.ai/inference/v1",
  "api_key_env": "FIREWORKS_API_KEY",
  "default_access_mode": "read-only"
}
```

Extra global instructions can be placed in `~/.cass/global.md`.

## Usage

```sh
cass [--model MODEL] [--base-url URL] [--api-key-env ENV] [--cwd PATH]
cass --resume <chat-id>
cass --resume
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

- `/model <model>`: switch the model for future turns
- `/resume <chat>`: resume a saved chat; chat autocomplete lists chats for the current directory
- `/status`: show current chat status

On exit Cass prints:

```text
Resume this chat with: cass --resume <id>
```

## Tools

Read-only mode allows `ls`, `read`, and `grep` within the launch cwd/`--cwd`.

Full-access mode additionally allows `write` and `edit`. Mutating tools use atomic writes where practical: Cass writes to a temporary file first, then renames it into place after validation/write success.

There is no shell/bash tool in v1.
