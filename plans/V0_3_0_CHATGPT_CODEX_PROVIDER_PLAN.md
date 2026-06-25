# v0.3.0 ChatGPT Codex Provider Implementation Plan

## Goal

This release adds a first-class `ChatGPT Codex` provider preset so users who are already signed in to Codex with a ChatGPT subscription can run Cassady without creating a separate API-key environment variable. The preset should call `https://chatgpt.com/backend-api/codex/responses` and resolve its bearer token from the local Codex auth config by default.

Success statement:

> A user who has already run `codex login` or signed in through the Codex app can select `ChatGPT Codex` in `cass login`, pass `cass check`, and send Cassady turns through their Codex subscription without copying tokens into Cassady config.

## Scope

### In scope

- Add `ChatGPT Codex` as a built-in provider preset in the setup/login catalog.
- Add a provider kind/client for the ChatGPT Codex responses endpoint rather than forcing it through `/chat/completions` URL construction.
- Read the default access token from the local Codex auth file, normally `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.
- Support the observed Codex auth shape with `tokens.access_token`, while keeping token values out of Cassady config, logs, check output, and error text.
- Prefer the Codex-configured model from `$CODEX_HOME/config.toml` when available, with a safe manual model fallback.
- Teach `cass check` to validate that local Codex auth is present and usable for the active `ChatGPT Codex` provider.
- Add docs explaining prerequisites, setup flow, token-source behavior, expiration troubleshooting, and the distinction between ChatGPT subscription access and API-key providers.
- Add focused tests with temporary Codex-home fixtures and mocked streaming responses.

### Out of scope

- Implementing Cassady's own browser OAuth/device-login flow for ChatGPT.
- Storing or refreshing ChatGPT/Codex tokens in Cassady-owned config files.
- Reverse engineering unrelated ChatGPT backend endpoints beyond the requested Codex responses endpoint.
- Guaranteeing compatibility if the private ChatGPT backend endpoint or Codex auth file format changes.
- Replacing OpenAI-compatible provider support or changing existing provider presets.
- Release tagging, packaging, or GitHub release creation.

## Context or Current State

Cassady's provider stack is currently centered on OpenAI-compatible chat completions:

- `src/setup.rs` owns the built-in provider catalog, setup/login prompts, model discovery via `GET {base_url}/models`, and writes to `providers.json`/`models.json`/`config.json`.
- `src/config.rs` defines `ProviderDefinition`, validates provider registries, resolves `api_key` values from literals or environment-variable references, and currently accepts only `kind = "openai-compatible"`.
- `src/agent.rs` constructs `OpenAiCompatibleProvider` directly from resolved config.
- `src/providers/openai_compatible.rs` appends `/chat/completions`, sends OpenAI-compatible chat payloads, and parses OpenAI-compatible streaming deltas.
- `docs/providers.md`, `docs/configuration.md`, `docs/commands.md`, `docs/workflows.md`, and `README.md` describe provider setup as API-key/environment-variable based.

The new preset differs in two important ways:

1. Authentication should come from Codex's local login state, not from a Cassady environment-variable API key.
2. The endpoint is a Codex-specific responses endpoint (`https://chatgpt.com/backend-api/codex/responses`), so Cassady needs an endpoint-specific provider client or a more general provider dispatch layer.

Local Codex auth is expected to live under Codex home, normally `~/.codex/auth.json`, with a shape like:

```json
{
  "auth_mode": "chatgpt",
  "tokens": {
    "access_token": "...",
    "refresh_token": "...",
    "account_id": "..."
  },
  "last_refresh": "..."
}
```

Cassady should treat this file as sensitive input: read it only when resolving the active provider token, never copy the access token into Cassady-owned JSON, and never print token contents.

## Design Principles

1. **Use Codex login state, do not own ChatGPT auth.** Cassady should integrate with an existing Codex login and tell users to run `codex login` or sign in to Codex when auth is missing or expired.
2. **Keep provider protocols explicit.** Do not pretend the ChatGPT Codex endpoint is OpenAI-compatible if it needs different URL construction, request shape, or stream parsing.
3. **Avoid token leakage.** Token values must not be stored in `~/.cass`, included in transcripts, surfaced in `cass check`, or embedded in test snapshots.
4. **Keep existing setup stable.** Existing providers should continue to use environment-variable API keys and `/models` discovery without extra Codex dependencies.
5. **Fail with clear recovery steps.** Missing Codex auth should produce actionable messages, not generic provider errors.

## Design

### Provider catalog and setup UX

Add a built-in catalog entry:

| Provider | Provider id | Kind | Endpoint | Token source |
| --- | --- | --- | --- | --- |
| ChatGPT Codex | `chatgpt-codex` | `chatgpt-codex` | `https://chatgpt.com/backend-api/codex/responses` | Local Codex auth |

In `cass login`/`cass setup`, selecting this provider should skip the normal `API key environment variable` prompt and instead show a prerequisite check:

```text
ChatGPT Codex uses your local Codex login.

✓ Found Codex auth at ~/.codex/auth.json
```

If the file or access token is missing:

```text
ChatGPT Codex needs a local Codex login.
Run `codex login` or sign in with the Codex app, then run `cass login` again.
```

Model selection should prefer, in order:

1. The `model` value from `$CODEX_HOME/config.toml` when present.
2. A known default Codex model constant only if the project already has a current default available.
3. Manual model id entry.

Do not call `GET /models` for `chatgpt-codex` unless a verified endpoint is added later; model discovery remains an OpenAI-compatible setup behavior.

### Provider configuration shape

Extend provider config without breaking existing files. One possible JSON shape is:

```json
{
  "id": "chatgpt-codex",
  "name": "ChatGPT Codex",
  "kind": "chatgpt-codex",
  "base_url": "https://chatgpt.com/backend-api/codex/responses",
  "auth": { "type": "codex_local" },
  "default_model": "gpt-5.5",
  "models": ["gpt-5.5"]
}
```

Implementation may choose an equivalent internal representation, but it should preserve these properties:

- Existing `api_key` string behavior remains valid for OpenAI-compatible providers.
- `chatgpt-codex` providers can omit environment-variable API keys.
- `cass check` can distinguish missing local Codex auth from missing API-key env vars.
- Serialized config does not contain the Codex access token.

### Codex auth resolution

Add a small resolver module, for example `src/codex_auth.rs`, with helpers like:

- `codex_home() -> PathBuf`: `$CODEX_HOME` when set, otherwise `~/.codex`.
- `codex_auth_path() -> PathBuf`: `$CODEX_AUTH_FILE` for tests/overrides when set, otherwise `{codex_home}/auth.json`.
- `load_codex_access_token() -> Result<CodexAccessToken>`: parse `tokens.access_token` and return a redacted/display-safe token wrapper.
- `check_codex_auth() -> CodexAuthStatus`: report path found, auth mode, access-token presence, optional JWT expiration, and recovery hints.

If the access token looks like a JWT, parse the `exp` claim without validating the signature so Cassady can warn or fail early when the token is expired. Token refresh itself should stay out of scope unless Codex exposes a stable documented local refresh interface.

Read the token at request time rather than caching it during setup. This allows a separate Codex process to refresh `auth.json` between Cassady turns.

### Provider dispatch

Refactor provider construction so `src/agent.rs` does not instantiate only `OpenAiCompatibleProvider`. A simple first step is an enum:

```rust
enum ProviderClient {
    OpenAiCompatible(OpenAiCompatibleProvider),
    ChatGptCodex(ChatGptCodexProvider),
}
```

Both variants should expose a common `complete(messages, tools, tx)` async method returning the existing `CompletionResult`. This preserves the current agent loop, tool execution, conversation storage, and TUI behavior.

### ChatGPT Codex responses client

Add a new provider module, for example `src/providers/chatgpt_codex.rs`, that:

- Posts to the exact configured endpoint, defaulting to `https://chatgpt.com/backend-api/codex/responses`.
- Sends `Authorization: Bearer <local Codex access token>`.
- Includes the active model and converted message/tool context in the endpoint's expected responses format.
- Streams assistant text into `AgentEvent::AssistantChunk`.
- Streams reasoning summaries into `AgentEvent::ReasoningChunk` only when the endpoint provides a safe reasoning summary field.
- Converts function/tool call deltas into Cassady `StoredToolCall` values.
- Converts Cassady tool results back into the endpoint's function-call-output input shape on the next turn.
- Redacts authentication details from non-success response errors.

The exact request/stream schema should be verified against the endpoint during implementation and captured in mocked fixtures. If the endpoint rejects a field used by OpenAI-compatible providers, keep the Codex payload minimal rather than adding compatibility shims that risk breaking the flow.

### Check and troubleshooting behavior

For active `chatgpt-codex` providers, `cass check` should report status like:

```text
✓ active provider: chatgpt-codex
✓ endpoint: https://chatgpt.com/backend-api/codex/responses
✓ Codex auth: ~/.codex/auth.json contains an access token
```

Failure should point to recovery:

```text
✗ Codex auth: no access token found in ~/.codex/auth.json
  Run `codex login` or sign in with the Codex app, then rerun `cass check`.
```

Do not print the token, account id, refresh token, or full auth JSON.

## Implementation Steps

1. Add the v0.3.0 roadmap entry and this implementation plan.
2. Extend provider config types/validation to support `kind = "chatgpt-codex"` and a non-env local Codex auth source while preserving existing OpenAI-compatible files.
3. Add `src/codex_auth.rs` for Codex home discovery, auth-file parsing, redacted status reporting, and optional JWT expiration checks.
4. Add `ChatGPT Codex` to `src/setup.rs` provider catalog and branch setup behavior so it skips API-key env prompts and `/models` discovery.
5. Refactor provider construction in `src/agent.rs` behind a small provider dispatch enum or trait.
6. Implement `src/providers/chatgpt_codex.rs` with endpoint-specific request conversion, streaming parsing, tool-call conversion, and redacted errors.
7. Update `cass check` so active and inactive provider checks understand Codex-local auth separately from environment-variable API keys.
8. Update README and bundled docs for the new preset, prerequisites, config example, troubleshooting, and known endpoint/auth caveats.
9. Add unit tests for config parsing/validation, Codex auth fixtures, setup catalog behavior, and provider dispatch.
10. Add mocked streaming tests for the ChatGPT Codex client, including text, tool calls, tool outputs, auth failures, and redaction.

## Tests

- `providers.json` with existing OpenAI-compatible providers still parses and validates.
- A `chatgpt-codex` provider with local Codex auth validates without `api_key`/env-var availability.
- Missing `~/.codex/auth.json` produces a clear `cass check` error for an active `chatgpt-codex` provider.
- A fixture `auth.json` with `tokens.access_token` resolves a token but redacts it in display and errors.
- Expired JWT-like access tokens are detected when possible and produce a recovery hint.
- Setup/login catalog includes `ChatGPT Codex` and skips the API-key env-var prompt for that provider.
- Model selection uses `$CODEX_HOME/config.toml` `model` when available, with manual fallback.
- Provider dispatch selects `ChatGptCodexProvider` only for `kind = "chatgpt-codex"`.
- Mocked Codex streaming responses produce assistant chunks and final `CompletionResult.content`.
- Mocked Codex function-call streams produce Cassady `StoredToolCall` values and accept subsequent tool output messages.
- Provider error messages never include access tokens, refresh tokens, or raw auth JSON.
- `cargo fmt` passes.
- `cargo test --locked --all-targets` passes when practical.

## Documentation

- Update `README.md` setup/provider sections with `ChatGPT Codex` as a subscription-backed option.
- Update `docs/providers.md` with the new preset, endpoint, token-source behavior, and private-endpoint caveat.
- Update `docs/configuration.md` with the extended provider schema and a safe example that uses local Codex auth.
- Update `docs/commands.md` and `docs/workflows.md` for `cass login`, `cass check`, and troubleshooting steps.
- Update `docs/troubleshooting.md` with missing/expired Codex auth, unsupported model, and backend endpoint failure guidance.

## Acceptance Criteria

- `cass login` offers `ChatGPT Codex` as a provider preset.
- Selecting `ChatGPT Codex` does not ask for an API-key environment variable by default.
- Cassady reads the access token from local Codex auth at request/check time and never stores that token under `~/.cass`.
- Active `chatgpt-codex` sessions call `https://chatgpt.com/backend-api/codex/responses` instead of appending `/chat/completions`.
- Normal OpenAI-compatible providers continue to work unchanged.
- `cass check` gives clear success/failure output for local Codex auth without leaking secrets.
- README and bundled docs explain the prerequisite of signing in to Codex first.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
