# Provider and Model Configuration Implementation Plan

## Goals

- Add `~/.cass/providers.json` as the source of truth for provider definitions.
- Add `~/.cass/models.json` for optional model metadata such as context length and max output tokens.
- Support API keys as either literal strings or environment-variable references in the form `"$PROVIDER_API_KEY"`.
- Document the config files well enough that a user can manually edit them or ask Cass, in full-access mode, to add/update providers and models by reading the bundled docs.
- Add `cass check` to validate config JSON syntax, schema, references, and basic operational readiness.

## Proposed file layout

Cass-managed/user-editable files under `~/.cass`:

- `config.json`: user preferences and default provider/model references only (no provider connection details or model metadata).
- `providers.json`: provider registry.
- `models.json`: model metadata registry.
- `docs/`: bundled read-only docs installed on startup.

Keep `config.json` for user preferences, but move provider connection details out of it. Continue to accept the existing `provider`, `model`, `base_url`, and `api_key_env` fields as a backward-compatible legacy path, with docs steering users to `default_provider`/`default_model` plus the new registry files.

## Proposed schemas

### `config.json`

```json
{
  "default_model": "accounts/fireworks/models/qwen3p7-plus",
  "default_access_mode": "read-only",
  "context_message_limit": 80,
  "model_tool_result_limit": 24000,
  "ui_tool_result_limit": 4000
}
```

Backward-compatible deprecated fields to continue accepting for now:

```json
{
  "base_url": "https://api.fireworks.ai/inference/v1",
  "api_key_env": "FIREWORKS_API_KEY"
}
```

### `providers.json`

Use an array to make manual edits straightforward and preserve room for provider-specific fields.

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

Initial provider fields:

- `id` required, unique stable identifier referenced by config defaults and models.
- `name` optional display name.
- `kind` required; initially only `"openai-compatible"` is supported.
- `base_url` required for `openai-compatible`.
- `api_key` required string. If it starts with `$`, resolve the remaining text as an environment variable name. Otherwise use it as a literal API key.
- `default_model` optional model to use when `config.json` omits `model`.
- `models` optional list of model ids associated with the provider.

### `models.json`

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
      "supports_streaming": true
    }
  ]
}
```

Initial model fields:

- `id` required model identifier sent to the provider.
- `provider` required provider id. Validate that it exists in `providers.json`.
- `display_name` optional human-friendly name.
- `context_length` optional positive integer.
- `max_output_tokens` optional positive integer.
- `supports_tools` optional boolean, defaults to `true`.
- `supports_streaming` optional boolean, defaults to `true`.

Deduplicate models by `(provider, id)`.

## Runtime behavior

1. On startup, ensure `~/.cass` exists as today.
2. If `providers.json` is missing, create a default file containing the current Fireworks provider with `"api_key": "$FIREWORKS_API_KEY"`.
3. If `models.json` is missing, create a default file containing metadata for the current default Fireworks model.
4. Load `config.json`, `providers.json`, and `models.json`.
5. Resolve the active model:
   - CLI `--model` overrides everything.
   - Else `config.default_model` (or legacy `config.model`).
   - Else selected provider `default_model`.
   - Else current built-in default.
6. Resolve the active provider:
   - Prefer `config.default_provider` when present.
   - Else infer from the selected model when `models.json` or provider model lists identify exactly one provider.
   - Else use the Fireworks default provider when available.
   - If legacy `base_url`/`api_key_env` are present and no registry provider is selected, synthesize a legacy `openai-compatible` provider for backward compatibility.
   - Otherwise fail with a clear config error that suggests running `cass check`.
7. Resolve API key:
   - `"$NAME"` means read env var `NAME`.
   - Empty env var names are invalid.
   - Literal strings are passed through unchanged.
   - Never print literal API key values in errors or check output.
8. Construct `OpenAiCompatibleProvider` from the resolved provider settings instead of raw `model/base_url/api_key_env` fields.

## CLI changes

Refactor `src/cli.rs` to support subcommands while preserving existing invocation forms:

```text
cass [--model MODEL] [--base-url URL] [--api-key-env ENV] [--cwd PATH]
cass --resume <chat-id>
cass --resume
cass check
```

Implementation sketch:

- Add `Command::Check` as an optional subcommand.
- Keep existing top-level flags for chat mode.
- In `app::run`, parse config, then if command is `Check`, run config checks and exit without entering the TUI.
- Exit code `0` when checks pass, non-zero when any error exists.

## `cass check` behavior

Initial check scope: config files only.

Checks:

- `config.json`, `providers.json`, and `models.json` parse as valid JSON when present.
- Files match expected schema/types and reject unknown required shapes.
- Required provider fields are present.
- Provider ids are unique.
- Provider `kind` is supported.
- `openai-compatible` providers have valid `base_url` and `api_key` strings.
- `$ENV_VAR` API key references have non-empty names.
- Active provider can be resolved from `default_provider`, selected model metadata, provider model lists, or a valid legacy fallback.
- Provider `default_model` and `models` entries can be matched against `models.json` when metadata exists.
- Model ids are unique within their provider scope and every model has a provider.
- Model numeric metadata is positive.
- Model `provider` references exist.
- Active provider API key resolves. For inactive providers, missing env vars should be warnings, not errors.

Output format example:

```text
Cass config check
✓ ~/.cass/config.json: valid
✓ ~/.cass/providers.json: valid (1 provider)
✓ ~/.cass/models.json: valid (1 model)
✓ active provider: fireworks
✓ active model: accounts/fireworks/models/qwen3p7-plus
✓ api key: FIREWORKS_API_KEY is set

All checks passed.
```

On errors, print each error with the file path and JSON path when possible.

## Code changes

### Config loading

- Extend `src/config.rs` or split into `src/config/` modules if it becomes large.
- Add structs:
  - `ProvidersFile`
  - `ProviderDefinition`
  - `ModelsFile`
  - `ModelDefinition`
  - `ResolvedProviderConfig`
  - `ResolvedModelMetadata`
- Add helper functions:
  - `load_or_create_default_provider_registry(root)`
  - `load_or_create_default_model_registry(root)`
  - `resolve_api_key(spec: &str) -> Result<String>`
  - `redact_api_key_for_display(spec: &str) -> String`
  - `validate_config_files(root, cli_overrides) -> CheckReport`
- Update `Config` to include resolved provider details, while keeping old fields during migration if needed.

### Provider construction

- Change `OpenAiCompatibleProvider::new` to accept a resolved settings struct:

```rust
pub struct OpenAiCompatibleSettings {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
}
```

- Update `agent::run_turn` to create the provider from `settings.config.resolved_provider`.
- Keep current request body behavior initially. Store model metadata for future context-management and output-token use.

### Check command

- Add a `src/check.rs` module for report types and rendering.
- `CheckReport` should hold errors and warnings separately.
- `cass check` should not start the TUI or create a conversation.
- Prefer deterministic output for tests.

### Documentation

Add bundled docs:

- `docs/configuration.md`: full schema examples, env-var API key behavior, manual editing steps, and examples prompts for asking Cass to add a provider/model.
- Update `docs/README.md` to link to `configuration.md`.
- Update top-level `README.md` Configure and Usage sections.

Include a user-facing example:

```text
Run cass in full-access mode and ask:
"Read ~/.cass/docs/configuration.md, then add an OpenAI-compatible provider named Together using TOGETHER_API_KEY and add model metadata for meta-llama/Llama-3.1-70B-Instruct-Turbo."
```

## Tests

Add tests for:

- Default `providers.json` and `models.json` creation in a temp Cass root.
- Loading current legacy `config.json` with `base_url` and `api_key_env` still works.
- `$ENV_VAR` API key resolution succeeds when set and fails clearly when missing.
- Literal API key strings are accepted and never appear in check output.
- Duplicate provider ids fail validation.
- Invalid JSON syntax fails validation with file path.
- Invalid provider/model references fail validation.
- `cass check` returns success for defaults and failure for invalid files.
- Docs install still includes the new configuration document.

## Migration/backward compatibility

- Do not break existing users with only `~/.cass/config.json`.
- Continue honoring `--base-url` and `--api-key-env`; internally convert them to provider overrides for the session.
- Mark `base_url` and `api_key_env` as deprecated in docs, but do not remove them yet.
- If both `providers.json` and legacy connection fields are present, `providers.json` wins unless CLI overrides are used.

## Suggested implementation order

1. Add registry structs, defaults, loading, API-key resolution, and validation helpers.
2. Add `cass check` CLI plumbing and report output.
3. Wire resolved provider settings into `OpenAiCompatibleProvider` and `agent::run_turn`.
4. Bootstrap missing `providers.json` and `models.json` with defaults.
5. Add docs and README updates.
6. Add/adjust tests.
7. Run `cargo fmt`, `cargo test`, and `cass check`.
