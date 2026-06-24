# v0.2.2 Onboarding and Setup Wizard Plan

## Goal

v0.2.2 focuses on first-run onboarding and quality of life when getting started with Cassady. A new user should be able to run `cass`, choose an OpenAI-compatible provider, choose the first model they want to use, configure the API key location, and immediately start a new Cassady session without reading documentation first.

Success statement:

> A new user can install Cassady, run `cass`, select a provider/model, pass setup validation, and start their first chat in under five minutes.

## Scope

### In scope

- Add an interactive setup wizard.
- Trigger setup automatically when Cass cannot resolve a usable active provider/model/API key.
- Add an explicit `cass setup` command to run setup on demand.
- Support only OpenAI-compatible providers.
- Offer a built-in provider catalog with common OpenAI-compatible providers.
- Support a custom OpenAI-compatible provider path.
- Prefer model discovery via the provider's OpenAI-compatible `/models` endpoint.
- Fall back to manual model id entry when model discovery fails or is skipped.
- Write/update `~/.cass/config.json`, `~/.cass/providers.json`, and `~/.cass/models.json` safely.
- Run validation equivalent to `cass check` after setup.
- Start a new chat session automatically after successful first-run setup.
- Improve `cass check` messaging where needed so setup errors are actionable.
- Update README and bundled docs after implementation.

### Out of scope

- Anthropic-native support or any non-OpenAI-compatible API protocol.
- Maintaining large hardcoded model catalogs for every provider.
- Multi-provider account management beyond selecting and saving the active provider/model.
- OAuth or browser-based provider authentication.
- Full TUI redesign.
- Changing agent/tool behavior after chat starts.

## Built-in Provider Catalog

The setup wizard should present these providers in this order:

| Provider | Provider id | Base URL | Suggested API key env var |
| --- | --- | --- | --- |
| OpenAI | `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| xAI | `xai` | `https://api.x.ai/v1` | `XAI_API_KEY` |
| Fireworks | `fireworks` | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` |
| Groq | `groq` | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` |
| OpenRouter | `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |
| OpenCode Zen | `opencode-zen` | `https://opencode.ai/zen/v1` | `OPENCODE_API_KEY` |
| OpenCode Go | `opencode-go` | `https://opencode.ai/zen/go/v1` | `OPENCODE_API_KEY` |
| Cerebras | `cerebras` | `https://api.cerebras.ai/v1` | `CEREBRAS_API_KEY` |
| Novita | `novita` | `https://api.novita.ai/v3/openai` | `NOVITA_API_KEY` |
| Together | `together` | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` |
| Custom OpenAI-compatible | user-entered | user-entered | user-entered |

Notes:

- The provider kind remains `openai-compatible` for every catalog entry.
- Use `Cerebras` spelling in UI and docs.
- OpenCode Zen and OpenCode Go share the suggested `OPENCODE_API_KEY` env var but have separate provider ids and base URLs.
- If a provider requires extra HTTP headers beyond Authorization in the future, defer that to a later provider configuration enhancement unless it blocks the standard OpenAI-compatible flow.

## User Experience

### First-run trigger

When the user runs:

```sh
cass
```

Cass should run normal config resolution first. If no usable active provider/model/API key exists, Cass should show a friendly setup prompt instead of dropping the user into a chat that will fail on the first request.

Example:

```text
Welcome to Cassady.

Cassady needs an OpenAI-compatible provider and model before starting your first chat.

Start setup now? [Y/n]
```

Default should be `Y`. If the user chooses `n`, print a concise next step:

```text
Run `cass setup` when you are ready.
```

### Explicit setup command

Add:

```sh
cass setup
```

This command should run the same wizard even if config already exists. If existing config is present, the wizard should make that clear and avoid destructive surprises:

```text
Cassady already has provider configuration.

This setup can update your active provider/model while preserving unrelated providers where possible.
Continue? [y/N]
```

Default should be `N` for already-configured setups.

### Provider selection

Prompt:

```text
Choose an OpenAI-compatible provider:

  1. OpenAI
  2. xAI
  3. Fireworks
  4. Groq
  5. OpenRouter
  6. OpenCode Zen
  7. OpenCode Go
  8. Cerebras
  9. Novita
  10. Together
  11. Custom OpenAI-compatible

Provider [3]:
```

Default can be Fireworks to preserve the current Cassady default.

The prompt should accept the number and, if easy to support, a case-insensitive provider id/name.

### Custom provider path

For custom providers, ask:

```text
Provider name:
Provider id:
Base URL:
API key environment variable:
```

Validation:

- Provider id must be non-empty and safe for config ids: lowercase letters, numbers, `_`, `-`, and `.` are acceptable.
- Base URL must be non-empty and should parse as an absolute HTTP/HTTPS URL.
- Env var name must be non-empty and should look like an environment variable name: uppercase letters, numbers, and `_` recommended. Do not make lowercase env vars impossible if the user insists, but warn.

### API key handling

The wizard should default to storing env-var references, not literal keys.

Prompt:

```text
API key environment variable [FIREWORKS_API_KEY]:
```

Then check whether the variable is set in the current process environment.

If set:

```text
✓ FIREWORKS_API_KEY is set
```

If missing:

```text
! FIREWORKS_API_KEY is not set in this shell.

Set it before starting a chat:
  export FIREWORKS_API_KEY=...

Continue setup anyway? [Y/n]
```

Default should be `Y`, because users may want Cassady to write config first and set env vars later. However, Cass should not auto-start the chat after setup if the selected active API key is still unavailable.

Literal API keys should not be the default path. If supported in v0.2.2, put it behind an explicit advanced option:

```text
Store a literal API key in providers.json? This is less secure than an environment variable. [y/N]
```

It is acceptable to defer literal-key entry from the wizard because current config files already support literal keys for advanced manual configuration.

### Model selection

After provider and API key env var are selected, attempt model discovery when the API key is available.

Request:

```http
GET {base_url}/models
Authorization: Bearer {api_key}
```

Expected OpenAI-compatible response shape:

```json
{
  "data": [
    { "id": "model-id" }
  ]
}
```

If discovery succeeds and returns models:

```text
Choose your first model:

  1. accounts/fireworks/models/qwen3p7-plus
  2. accounts/fireworks/models/deepseek-v3
  3. Enter model id manually

Model:
```

Model list behavior:

- Sort models alphabetically unless provider order is meaningful and preserved from response.
- Cap displayed models to a reasonable amount, e.g. first 50, with manual entry always available.
- If filtering/search is easy in a future interactive UI, defer it. A simple numbered list is enough for v0.2.2.

If discovery fails, API key is missing, or response is unsupported:

```text
Cassady could not fetch models from this provider.

Enter the model id you want to use:
```

Manual model id must be non-empty.

### Model metadata defaults

The wizard should create a minimal useful model metadata entry.

Defaults for discovered or manually entered models:

```json
{
  "id": "selected-model-id",
  "provider": "selected-provider-id",
  "supports_tools": true,
  "supports_streaming": true,
  "reasoning": {
    "supported": true,
    "required": false,
    "default_effort": "medium",
    "request_format": "reasoning_effort"
  }
}
```

OpenAI-compatible providers vary in reasoning support. Since Cassady already allows model metadata edits, v0.2.2 can choose a pragmatic default but should let the user opt out if the setup flow asks advanced questions.

Recommended v0.2.2 simple path:

```text
Does this model support tool calls? [Y/n]
Does this model support reasoning effort controls? [Y/n]
```

Defaults:

- Tool calls: `Y`
- Reasoning controls: `Y` for continuity with current defaults, or `n` if early testing shows many providers reject reasoning fields.

If the user says reasoning is not supported, write:

```json
"reasoning": { "supported": false }
```

If the user says tool calls are not supported, write `"supports_tools": false` and warn:

```text
! Cassady works best with models that support tool calls.
```

### Config write behavior

After selection, write/update:

- `~/.cass/providers.json`
- `~/.cass/models.json`
- `~/.cass/config.json`

Rules:

- Preserve unrelated existing providers and models where possible.
- Upsert the selected provider by id.
- Upsert the selected model by id.
- Set active preferences in `config.json`:
  - `default_provider`: selected provider id
  - `default_model`: selected model id
  - keep existing user preferences such as `default_access_mode`, tool result limits, context settings, and `show_reasoning` unless setup explicitly changes them.
- Do not write API key literals by default. Use `"$ENV_VAR_NAME"`.
- Use pretty JSON formatting.
- Avoid printing literal API key values if literal keys are ever supported.

Example provider entry:

```json
{
  "id": "groq",
  "name": "Groq",
  "kind": "openai-compatible",
  "base_url": "https://api.groq.com/openai/v1",
  "api_key": "$GROQ_API_KEY",
  "default_model": "llama-3.3-70b-versatile",
  "models": [
    "llama-3.3-70b-versatile"
  ]
}
```

Example config entry:

```json
{
  "default_provider": "groq",
  "default_model": "llama-3.3-70b-versatile",
  "default_access_mode": "read-only"
}
```

## Post-setup validation and session start

After writing config, run the same validation used by `cass check`.

If validation passes and active API key is available:

```text
Setup complete.

Starting your first Cassady session...
```

Then start a new chat session automatically.

If validation passes but API key is missing:

```text
Setup saved, but your API key is not available in this shell.

Set it with:
  export FIREWORKS_API_KEY=...

Then run:
  cass
```

Do not start a chat automatically in this case.

If validation fails:

```text
Setup was saved, but Cassady is not ready yet.

<rendered check errors>

Run `cass setup` to try again or edit ~/.cass/config.json manually.
```

Do not start a chat automatically.

## `cass check` Quality-of-Life Improvements

`cass check` should remain non-interactive, but its output should be more onboarding-oriented.

Add or verify:

- Active provider id.
- Active provider base URL.
- Active model id.
- API key env var name and whether it is set.
- Next-step suggestions when setup is incomplete.
- A hint to run `cass setup` when config is missing or invalid.

Example failure:

```text
Cass config check

✓ ~/.cass/providers.json: valid (1 provider)
✓ ~/.cass/models.json: valid (1 model)
✓ active provider: fireworks
✓ active model: accounts/fireworks/models/qwen3p7-plus
✗ api key: environment variable `FIREWORKS_API_KEY` is not set

Next step:
  export FIREWORKS_API_KEY=...

Then run:
  cass check
  cass

Config check failed.
```

## CLI/API Design Notes

Suggested CLI enum addition:

```rust
pub enum Command {
    Check,
    Setup,
}
```

Suggested module:

```text
src/setup.rs
```

Potential responsibilities:

- Provider catalog definitions.
- Interactive prompt helpers.
- Provider/model selection.
- Model discovery.
- Config upsert/write.
- Post-setup validation result.

Keep setup separate from the TUI chat app so it can run in plain terminal mode before ratatui/crossterm takes over.

## Error Handling

- Network failures during model discovery should not fail setup; fall back to manual model id entry.
- Invalid user input should re-prompt with concise guidance.
- Config write failures should fail setup and print the file path and error.
- Validation failures after writes should be printed clearly.
- Do not reveal literal API keys in errors or logs.
- If stdout is non-interactive in the future, setup can fail with a message telling the user to run interactively. v0.2.2 does not need a full non-interactive setup mode.

## Testing Plan

Add unit/integration coverage for:

- Provider catalog contains the expected providers, ids, URLs, and env vars.
- Custom provider validation accepts valid ids/base URLs and rejects empty/invalid required fields.
- Config upsert preserves unrelated providers/models.
- Config upsert updates selected provider/model and active defaults.
- `cass check` reports missing active API key with actionable next steps.
- Model discovery parses OpenAI-compatible `/models` responses.
- Model discovery failure falls back to manual model entry without failing setup.
- First-run app path invokes setup when active config cannot be used.
- Successful setup with available API key proceeds to a new session.
- Successful setup with missing API key saves config but does not start a chat.

If interactive stdin/stdout tests are too heavy, isolate the wizard behind an input/output trait so scripted tests can provide answers and capture prompts.

## Documentation Updates

After implementation, update:

- `README.md`
  - Add `cass setup`.
  - Describe first-run provider/model selection.
  - Update access mode docs to include `workspace-edit` if still missing.
- `docs/configuration.md`
  - Add setup wizard section.
  - Document built-in provider catalog.
  - Clarify env-var API key handling.
- `docs/README.md`
  - Link to setup/configuration docs.
- Any release notes/changelog file if one is added before v0.2.2.

## Acceptance Criteria

- Running `cass` on a fresh machine/config prompts for setup instead of failing in-chat.
- The user can select OpenAI, xAI, Fireworks, Groq, OpenRouter, OpenCode Zen, OpenCode Go, Cerebras, Novita, Together, or a custom OpenAI-compatible provider.
- The user can select a discovered model or enter a model id manually.
- Setup writes valid Cassady config files.
- Setup validates the result and only starts a new chat when the active API key is available.
- `cass setup` can be run explicitly.
- `cass check` provides actionable next steps for incomplete setup.
- No Anthropic-native or non-OpenAI-compatible provider path appears in setup.
