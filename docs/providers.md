# Providers and models

Cassady supports OpenAI-compatible providers plus a built-in `ChatGPT Codex` provider preset. A provider supplies the endpoint and authentication source; a model entry supplies metadata for one model id used with that provider.

## Built-in setup catalog

The setup wizard offers these provider templates:

| Provider | Provider id | Base URL / endpoint | Suggested auth source |
| --- | --- | --- | --- |
| OpenAI | `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| ChatGPT Codex | `chatgpt-codex` | `https://chatgpt.com/backend-api/codex/responses` | local Codex auth |
| xAI | `xai` | `https://api.x.ai/v1` | `XAI_API_KEY` |
| Fireworks | `fireworks` | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` |
| Groq | `groq` | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` |
| OpenRouter | `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |
| OpenCode Zen | `opencode-zen` | `https://opencode.ai/zen/v1` | `OPENCODE_API_KEY` |
| OpenCode Go | `opencode-go` | `https://opencode.ai/zen/go/v1` | `OPENCODE_API_KEY` |
| Cerebras | `cerebras` | `https://api.cerebras.ai/v1` | `CEREBRAS_API_KEY` |
| Novita | `novita` | `https://api.novita.ai/v3/openai` | `NOVITA_API_KEY` |
| Together | `together` | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` |

There is also a custom OpenAI-compatible option. Custom setup asks for provider name, provider id, base URL, API key environment variable, and first model id.

## Login and logout

Use `cass login` to add or update provider configuration from the shell. Inside an idle chat, `/login` opens the same flow and reloads the active provider/model afterward.

Use `cass logout` or `/logout` to remove saved provider entries from Cassady config. Logout also removes model metadata entries associated with the removed providers and repairs active defaults when other providers remain. It does not delete environment variables, shell profile exports, API keys stored elsewhere, or external provider accounts.

## ChatGPT Codex preset

`ChatGPT Codex` is for users who have already signed in with Codex. Run `codex login` or sign in with the Codex app first, then select `ChatGPT Codex` in `cass login` or `cass setup`.

Cassady reads the bearer token from `$CODEX_HOME/auth.json` or `~/.codex/auth.json` at check/request time. It does not copy the access token or refresh token into `~/.cass`, and `cass check` redacts secret values. Setup prefers the `model` value from `$CODEX_HOME/config.toml` when present and otherwise offers a default/manual model id.

This preset uses `kind: "chatgpt-codex"` and posts to `https://chatgpt.com/backend-api/codex/responses`. That ChatGPT backend endpoint and Codex auth file format are outside Cassady's control, so users may need to update Cassady if they change.

## Model discovery

For OpenAI-compatible providers, when the selected API key environment variable is available, setup tries:

```text
GET {base_url}/models
```

If the provider returns model ids, setup lets you choose one. If discovery fails, setup offers a retry and then falls back to manual model entry. Some OpenAI-compatible providers do not expose `/models` or require different permissions; manual entry is normal in that case.

## Custom provider requirements

A custom provider should expose OpenAI-compatible chat completions behavior at the configured base URL. Cassady may use:

- streamed assistant text;
- tool call requests and tool results;
- optional reasoning fields or reasoning request controls;
- optional `/models` discovery during setup.

Provider protocols that are not OpenAI-compatible are supported only when Cassady has an explicit provider kind for them, such as `chatgpt-codex`.

## Provider vs model metadata

`providers.json` answers: how does Cassady connect?

- provider id;
- base URL;
- API key reference or local auth source;
- optional default model;
- optional list of associated model ids.

`models.json` answers: what does this model support?

- model id sent to the provider;
- owning provider id;
- display name;
- context length and max output tokens;
- tool and streaming support;
- reasoning support and request format;
- fast-mode support.

`config.json` selects active defaults, such as `default_provider`, `default_model`, and `default_access_mode`.

## Reasoning metadata

Reasoning metadata controls how the runtime reasoning effort behaves:

- `supported: false`: reasoning effort stays `off`.
- `required: true`: `Tab` cycles through `low`, `medium`, and `high` without `off`.
- `default_effort`: starting effort for the model.
- `request_format: "reasoning_effort"`: sends a top-level `reasoning_effort` string.
- `request_format: "reasoning_object"`: sends a `reasoning` object with an effort.

Reasoning display is separate. `show_reasoning` controls whether provider-streamed reasoning is visible in the transcript; press `Ctrl-Shift-R` or `Ctrl-R` to toggle it at runtime.

## Fast-mode metadata

Fast mode has two parts:

- `default_fast_mode` in `config.json`: the user's saved preference.
- `fast_mode.supported` in `models.json`: whether the active provider/model can honor that preference.

In v0.3.2, Cassady sends fast-mode requests only for `ChatGPT Codex`. Setup marks ChatGPT Codex model entries as fast-capable. OpenAI-compatible and custom model entries default to unsupported, so `/fast` can remember the preference without sending provider-specific fields.

## Switching models

Use one of these approaches:

```sh
cass --model MODEL
```

or inside a chat:

```text
/model MODEL
```

The in-chat model autocomplete lists entries from `~/.cass/models.json`. Switching the model also updates the default provider, default model, and reasoning effort in `config.json` for future sessions. If fast mode is preferred, Cassady recomputes whether it is active after the switch.

## Health checks

Run:

```sh
cass check
```

This confirms that the active provider and model resolve. For OpenAI-compatible providers it checks API key environment variables; missing inactive-provider keys are warnings and missing active-provider keys are errors. For `ChatGPT Codex`, it checks that local Codex auth contains an access token and prints recovery steps if not.
