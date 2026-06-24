# Configuration

Cass reads user-editable config files from `~/.cass`.

- `config.json`: user preferences, such as the default model and access mode.
- `providers.json`: provider connection definitions.
- `models.json`: model metadata.

Cass creates `providers.json` and `models.json` automatically if they are missing. The default provider is Fireworks.

## `config.json`

`config.json` should contain preferences only. Provider connection details belong in `providers.json`; model metadata belongs in `models.json`.

Example:

```json
{
  "default_model": "accounts/fireworks/models/qwen3p7-plus",
  "default_access_mode": "read-only",
  "context_message_limit": 80,
  "model_tool_result_limit": 24000,
  "ui_tool_result_limit": 4000,
  "show_reasoning": false
}
```

Fields:

- `default_provider`: optional provider id from `providers.json`. If omitted, Cass infers the provider from `default_model` when possible.
- `default_model`: optional model id to use by default.
- `default_access_mode`: `"read-only"` or `"full-access"`.
- `context_message_limit`: optional legacy upper bound for recent non-system messages. Cass primarily budgets context from model metadata (`context_length` and `max_output_tokens`), compacts older tool outputs when needed, and trims only along valid tool-call boundaries.
- `model_tool_result_limit`: optional max bytes of tool output sent back to the model.
- `ui_tool_result_limit`: optional max bytes of tool output shown in the UI unless full output is toggled.
- `show_reasoning`: optional boolean, defaults to `false`. Shows provider-streamed reasoning in the transcript. Reasoning is persisted and sent back in future model context using the provider's reasoning field, such as `reasoning_content` or `reasoning`.

Deprecated compatibility fields from older Cass versions are still accepted: `provider`, `model`, `base_url`, and `api_key_env`. Prefer moving provider connection details to `providers.json`.

## `providers.json`

Example:

```json
{
  "providers": [
    {
      "id": "fireworks",
      "name": "Fireworks",
      "kind": "openai-compatible",
      "base_url": "https://api.fireworks.ai/inference/v1",
      "api_key": "$FIREWORKS_API_KEY",
      "default_model": "accounts/fireworks/models/qwen3p7-plus",
      "models": [
        "accounts/fireworks/models/qwen3p7-plus"
      ]
    }
  ]
}
```

Fields:

- `id`: required unique provider id.
- `name`: optional display name.
- `kind`: required provider kind. Currently only `"openai-compatible"` is supported.
- `base_url`: required OpenAI-compatible API base URL.
- `api_key`: required string. Use either a literal key or an environment-variable reference like `"$FIREWORKS_API_KEY"`.
- `default_model`: optional model id to use when no default model is configured.
- `models`: optional list of model ids associated with this provider.

Only strings that start with `$` are resolved as environment variables. Cass does not expand partial strings or `${NAME}` syntax.

## `models.json`

Example:

```json
{
  "models": [
    {
      "id": "accounts/fireworks/models/qwen3p7-plus",
      "provider": "fireworks",
      "display_name": "Qwen 3p7 Plus",
      "context_length": 262144,
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
  - `supported`: optional boolean, defaults to `true`. Set to `false` for models that do not accept reasoning controls.
  - `required`: optional boolean, defaults to `false`. If `true`, Cass will not cycle reasoning effort to `off`.
  - `default_effort`: optional `off`, `low`, `medium`, or `high`; defaults to `medium`. Cannot be `off` when `required` is `true`.
  - `request_format`: optional `reasoning_effort` or `reasoning_object`; defaults to `reasoning_effort`. `reasoning_effort` sends a top-level `"reasoning_effort": "medium"`; `reasoning_object` sends `"reasoning": { "effort": "medium" }`.

Reasoning effort is a runtime per-turn setting. Press `Tab` to cycle it while idle. For models with reasoning metadata, the default effort is `medium` unless overridden by `default_effort`; for models without metadata, reasoning starts `off`.

## Check configuration

Run:

```sh
cass check
```

This validates JSON syntax, expected schema, duplicate provider/model ids, model/provider references, active provider/model resolution, and API key environment-variable availability. Missing API keys for inactive providers are warnings; a missing active provider API key is an error.

## Ask Cass to edit config

Run Cass in full-access mode and ask it to read these docs before editing:

```text
Read ~/.cass/docs/configuration.md, then add an OpenAI-compatible provider named Together using TOGETHER_API_KEY and add model metadata for meta-llama/Llama-3.1-70B-Instruct-Turbo.
```

After Cass edits the files, run `cass check`.
