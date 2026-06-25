# Configuration

Cassady reads user-editable config files from `~/.cass`.

- `config.json`: user preferences, active defaults, and compatibility fields.
- `providers.json`: provider connection definitions.
- `models.json`: model metadata.
- `global.md`: optional global instructions included in new chat system prompts when they fit the active request; they cannot override access modes, tool denials, approvals, or workspace boundaries.
- `conversations/`: saved JSONL chats.
- `docs/`: bundled docs installed from the current binary.

Cassady creates `providers.json` and `models.json` automatically if they are missing. The default provider is Fireworks.

## Setup wizard

Run:

```sh
cass setup
```

Cassady also offers setup automatically when `cass` cannot resolve a usable active provider, model, or authentication source before starting a chat.

For everyday provider management, `cass login` opens the same provider configuration flow with login-oriented wording. Inside an idle chat, `/login` temporarily opens that flow and reloads the active provider/model after it closes.

To remove saved provider configuration, run `cass logout` or type `/logout` while idle. Logout removes selected providers from `providers.json` and removes their associated entries from `models.json`. It does not remove environment variables, local shell profile exports, or external provider accounts.

The wizard uses keyboard prompts: `↑`/`↓` moves through choices, `Space` selects providers in the multi-select screen, and `Enter` submits. Text fields use the same prompt style instead of falling back to plain line input.

The wizard supports configuring multiple providers at once. If more than one provider is configured, setup asks which one should be active first. For OpenAI-compatible providers, if the selected API key environment variable is set, Cassady tries to fetch models from `GET {base_url}/models` and lets you choose one. If discovery fails, it offers a retry before falling back to manual model entry. If the API key is not set, setup asks for a model id manually. For `ChatGPT Codex`, setup skips API-key prompts and reads model defaults from local Codex config when available.

Setup stores API keys as environment-variable references such as `"$OPENAI_API_KEY"` by default for OpenAI-compatible providers. `ChatGPT Codex` stores no API key in `~/.cass`; it reads local Codex auth at check/request time. After setup, Cassady writes/updates `config.json`, `providers.json`, and `models.json`, validates them, and starts a chat only when the active authentication source is available.

## `config.json`

`config.json` should contain preferences only. Provider connection details belong in `providers.json`; model metadata belongs in `models.json`.

Example:

```json
{
  "default_provider": "openai",
  "default_model": "gpt-4.1",
  "default_reasoning_effort": "medium",
  "default_access_mode": "read-only",
  "context_message_limit": 80,
  "model_tool_result_limit": 24000,
  "ui_tool_result_limit": 4000,
  "show_reasoning": false,
  "confirm_destructive_operations": false
}
```

Fields:

- `default_provider`: optional provider id from `providers.json`. If omitted, Cassady infers the provider from `default_model` when possible.
- `default_model`: optional model id to use by default.
- `default_reasoning_effort`: optional `off`, `low`, `medium`, or `high`, clamped to model metadata.
- `default_access_mode`: `"read-only"`, `"workspace-edit"`, or `"full-access"`.
- `context_message_limit`: optional legacy upper bound for recent non-system messages. Cassady primarily budgets context from model metadata and trims along valid tool-call boundaries.
- `model_tool_result_limit`: optional max bytes of tool output sent back to the model.
- `ui_tool_result_limit`: optional max bytes of tool output shown in the UI unless full output is toggled.
- `show_reasoning`: optional boolean, defaults to `false`. Shows provider-streamed reasoning in the transcript.
- `confirm_destructive_operations`: optional compatibility preference currently stored in config.

Deprecated compatibility fields from older Cassady versions are still accepted: `provider`, `model`, `base_url`, and `api_key_env`. Prefer moving provider connection details to `providers.json`.

## `providers.json`

Example:

```json
{
  "providers": [
    {
      "id": "openai",
      "name": "OpenAI",
      "kind": "openai-compatible",
      "base_url": "https://api.openai.com/v1",
      "api_key": "$OPENAI_API_KEY",
      "default_model": "gpt-4.1",
      "models": ["gpt-4.1"]
    }
  ]
}
```

Fields:

- `id`: required unique provider id.
- `name`: optional display name.
- `kind`: required provider kind. Supported values are `"openai-compatible"` and `"chatgpt-codex"`.
- `base_url`: required API base URL or endpoint. `chatgpt-codex` uses `https://chatgpt.com/backend-api/codex/responses`.
- `api_key`: required for `openai-compatible` providers. Use either a literal key or an environment-variable reference like `"$OPENAI_API_KEY"`. Omit it for `chatgpt-codex`; that provider reads local Codex auth instead.
- `default_model`: optional model id used when no default model is configured.
- `models`: optional list of model ids associated with this provider.

Only strings that start with `$` are resolved as environment variables. Cassady does not expand partial strings or `${NAME}` syntax.

`ChatGPT Codex` example:

```json
{
  "providers": [
    {
      "id": "chatgpt-codex",
      "name": "ChatGPT Codex",
      "kind": "chatgpt-codex",
      "base_url": "https://chatgpt.com/backend-api/codex/responses",
      "default_model": "gpt-5.5",
      "models": ["gpt-5.5"]
    }
  ]
}
```

Run `codex login` or sign in with the Codex app before using this provider. Cassady reads `$CODEX_HOME/auth.json` or `~/.codex/auth.json` and does not store the Codex access token in `~/.cass`.

## `models.json`

Example:

```json
{
  "models": [
    {
      "id": "gpt-4.1",
      "provider": "openai",
      "display_name": "GPT-4.1",
      "context_length": 1047576,
      "max_output_tokens": 32768,
      "supports_tools": true,
      "supports_streaming": true,
      "reasoning": {
        "supported": true,
        "required": false,
        "default_effort": "medium",
        "request_format": "reasoning_effort"
      }
    }
  ]
}
```

Fields:

- `id`: required model id sent to the provider.
- `provider`: required provider id from `providers.json`.
- `display_name`: optional human-friendly name.
- `context_length`: optional positive integer.
- `max_output_tokens`: optional positive integer.
- `supports_tools`: optional boolean, defaults to `true`.
- `supports_streaming`: optional boolean, defaults to `true`.
- `reasoning`: optional object. Defaults to reasoning support enabled with medium effort for model entries.
  - `supported`: optional boolean, defaults to `true`.
  - `required`: optional boolean, defaults to `false`.
  - `default_effort`: optional `off`, `low`, `medium`, or `high`; defaults to `medium`. Cannot effectively be `off` when `required` is `true`.
  - `request_format`: optional `reasoning_effort` or `reasoning_object`; defaults to `reasoning_effort`.

Reasoning effort is a runtime per-turn setting. Press `Tab` to cycle it while idle. Provider-streamed reasoning is persisted and sent back in future model context using the provider's reasoning field, such as `reasoning_content` or `reasoning`.

## Precedence

- CLI access-mode flags override `default_access_mode` for the current session.
- `--model` overrides the configured default model for the current session.
- `--base-url` overrides the active provider base URL for the current session.
- `--api-key-env ENV` makes the active provider read `$ENV` for the current session.
- `config.json` preferences override built-in defaults.
- Provider defaults in `providers.json` are used when no configured model is selected.
- Environment variables provide the actual API key value when `api_key` starts with `$`.

## Check configuration

Run:

```sh
cass check
```

This validates JSON syntax, expected schema, duplicate provider/model ids, model/provider references, active provider/model resolution, and authentication availability. Missing API keys for inactive OpenAI-compatible providers are warnings; a missing active provider API key is an error. For `ChatGPT Codex`, missing or expired local Codex auth is an active-provider error.

When setup is incomplete, `cass check` prints actionable next steps such as:

```text
export PROVIDER_API_KEY=...
cass check
cass
```

## Safe manual editing

1. Edit one file at a time.
2. Keep provider ids and model provider references in sync.
3. Prefer API key env references over literal keys for OpenAI-compatible providers; do not paste Codex tokens into Cassady config.
4. Prefer `cass login` and `cass logout` for routine provider changes.
5. Run `cass check` before starting a chat.

Invalid JSON, unknown fields, duplicate ids, and missing provider/model links are reported by `cass check` with the file that failed.
