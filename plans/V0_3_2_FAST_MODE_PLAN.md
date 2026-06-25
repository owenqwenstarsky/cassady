# v0.3.2 Fast Mode Implementation Plan

## Goal

v0.3.2 adds a `/fast` command that lets users opt into faster provider inference when the active provider/model supports it. The setting should feel like a user preference, but the runtime state should be capability-aware: fast mode is shown as enabled only when the current provider/model can actually honor it.

Success statement:

> A ChatGPT Codex user can type `/fast`, see fast mode enabled for Codex models that support it, switch to an unsupported provider/model and see fast mode become unavailable, then switch back and have the preference apply again.

## Scope

### In scope

- Add an idle-only `/fast` local command that toggles the user's fast-mode preference.
- Persist the preference in Cassady config so it survives new chats and restarts.
- Add provider/model capability metadata that determines whether fast mode is currently active.
- Implement fast-mode request support for `ChatGPT Codex` first.
- Keep unsupported providers/models explicit: the preference can remain on, but the UI/status should say fast mode is unavailable rather than enabled.
- Update `/status`, the bottom/status line, command autocomplete/help, README, and bundled docs.
- Add focused tests for command parsing, persistence, capability gating, provider request shaping, and model switching.

### Out of scope

- Adding fast-mode support for OpenAI-compatible providers in v0.3.2.
- Guessing provider-specific fast-mode request fields without verified behavior.
- Adding latency benchmarking, automatic mode selection, or per-turn speed/quality controls beyond the `/fast` toggle.
- Changing the default model selection flow except to record fast-mode capability for known built-in presets.
- Treating fast mode as a quality guarantee; providers may still vary in latency and output behavior.

## Context or Current State

Cassady already has several runtime preferences and model/provider capability paths that should guide this work:

- `src/app.rs` parses local slash commands such as `/model`, `/login`, `/logout`, and `/status`, and already restricts provider/model changes to idle state.
- `src/config.rs` persists default model and reasoning effort in `config.json` and stores model metadata in `models.json`.
- `ModelDefinition` already includes capability-like metadata such as `supports_tools`, `supports_streaming`, and `reasoning`.
- `ProviderClient::from_config` in `src/providers/mod.rs` dispatches by provider kind to `OpenAiCompatibleProvider` or `ChatGptCodexProvider`.
- `src/providers/chatgpt_codex.rs` is the first provider where fast mode should affect the outgoing request.
- `/model` already reloads model metadata and resets reasoning effort based on the new model.
- `/status` currently reports chat id, model, mode, cwd, records, and current status.

The important product behavior is that "fast mode preference" and "fast mode active" are different states:

- Preference: whether the user wants fast mode when possible.
- Active: whether the current provider/model supports fast mode and the preference is enabled.

## Design Principles

1. **Preference stays stable, capability controls activation.** Switching to an unsupported provider/model should not erase the user's preference; it should only make fast mode inactive until support is available again.
2. **Provider-specific request details stay behind provider clients.** The agent loop should pass a normalized fast-mode flag; each provider decides whether and how to encode it.
3. **UI wording must distinguish enabled from unavailable.** Avoid showing "fast mode enabled" when Cassady cannot send a fast-mode request for the active provider/model.
4. **Extend metadata, do not hardcode every check in the TUI.** Use provider/model capability helpers so future providers can add fast mode without rewriting command handling.
5. **Keep unsupported behavior quiet and compatible.** Existing OpenAI-compatible providers should continue working unchanged and should not receive unknown request fields.

## Design

### User model

Add a user preference to `config.json`:

```json
{
  "default_fast_mode": true
}
```

Suggested behavior:

- Missing `default_fast_mode` defaults to `false`.
- `/fast` toggles the preference while idle.
- `/fast on` and `/fast off` may be supported if easy, but the minimum required command is the toggle form.
- The preference is persisted immediately, similar to last-used model/reasoning persistence.
- The active state is recomputed whenever config, provider, model, or model metadata changes.

Status examples:

```text
fast mode: enabled
fast mode: unavailable for provider fireworks
fast mode: off
```

If the user toggles fast mode on while using an unsupported provider:

```text
fast mode preference on; unavailable for this provider/model
```

If the user later switches to a supported Codex model, the UI should show:

```text
fast mode enabled
```

### Capability model

Add a small capability representation, preferably on model metadata with provider-kind fallback:

```json
{
  "id": "gpt-5.5",
  "provider": "chatgpt-codex",
  "fast_mode": {
    "supported": true
  }
}
```

Rules:

- `fast_mode.supported` defaults to `false` unless a provider-specific built-in preset intentionally marks it true.
- For the `ChatGPT Codex` built-in setup path, saved Codex model metadata should mark fast mode as supported when the implementation can send the fast-mode request for that provider.
- Manual custom providers and discovered OpenAI-compatible models should default to unsupported.
- If a provider supports fast mode for all models but model metadata is missing, provider-specific capability fallback may return supported. Keep that fallback in config/provider capability helpers, not in UI string matching.

Add helper APIs along these lines:

```rust
pub struct FastModeState {
    pub preferred: bool,
    pub supported: bool,
    pub active: bool,
    pub unavailable_reason: Option<String>,
}

impl Config {
    pub fn fast_mode_state(&self) -> FastModeState;
}
```

`active` should be exactly `preferred && supported`.

### Provider request behavior

Thread the active fast-mode boolean through the provider settings:

```rust
ProviderClient::from_config(&config, reasoning_effort, fast_mode_active)
```

or include it in a runtime options struct if that is cleaner:

```rust
pub struct ProviderRuntimeOptions {
    pub reasoning_effort: ReasoningEffort,
    pub fast_mode: bool,
}
```

`ChatGptCodexProvider` should encode fast mode using the verified Codex responses request shape. During implementation, verify the exact field against the current Codex behavior and capture the resulting request body in tests. The plan intentionally does not prescribe a speculative field name.

OpenAI-compatible providers should ignore the setting until support is explicitly added. They must not receive experimental Codex-only fields.

### UI and command behavior

Add `LocalCommand::Fast(FastModeCommand)` in `src/app.rs`.

Minimum command behavior:

```text
/fast
```

Recommended optional forms:

```text
/fast on
/fast off
/fast status
```

Command handling:

- Only allow changes while idle.
- Persist the preference to `config.json`.
- Recompute active state from the current provider/model after toggling.
- Append or show a concise status message.
- Include `/fast` in autocomplete/local command help.

Update `/status` to include both preference and active state, for example:

```text
fast: enabled
```

or:

```text
fast: preferred, unavailable for provider fireworks
```

The bottom/status line should include a compact signal only when useful:

- `fast` when active.
- No `fast` label when off.
- Optional `fast unavailable` only immediately after toggling or in `/status`, to avoid clutter.

### Model and provider switching

When `/model` changes the active model:

- Reload `config.model_metadata` as today.
- Recompute reasoning effort as today.
- Recompute fast-mode state.
- Show the model status with fast-mode state when the preference is on.

Expected examples:

```text
model: gpt-5.5 · fast enabled
model: accounts/fireworks/models/qwen3p7-plus · fast unavailable
```

When `/login` or `/logout` updates provider config:

- Reload config as today.
- Preserve `default_fast_mode`.
- Recompute fast-mode state for the new active provider/model.

### Configuration and docs

Update docs to describe:

- `default_fast_mode` in `config.json`.
- `fast_mode.supported` in `models.json`.
- `/fast` command behavior.
- Provider support status: v0.3.2 supports fast mode only for `ChatGPT Codex`.
- The distinction between fast-mode preference and active fast-mode support.

## Implementation Steps

1. Extend config types in `src/config.rs`:
   - Add `default_fast_mode: Option<bool>` to the config file representation.
   - Add `fast_mode` metadata to model definitions.
   - Add a `FastModeState` helper that computes preferred/supported/active.
2. Add persistence helpers:
   - Save fast-mode preference without disturbing unrelated config fields.
   - Ensure existing `save_last_used` behavior preserves the new field.
3. Mark built-in `ChatGPT Codex` model metadata as fast-mode capable during setup/login when the provider implementation supports it.
4. Add provider runtime options and pass `fast_mode_state.active` into `ProviderClient` construction from `src/agent.rs`.
5. Implement Codex request support in `src/providers/chatgpt_codex.rs` using the verified fast-mode request shape.
6. Keep `OpenAiCompatibleProvider` behavior unchanged and add tests proving it does not receive fast-mode fields.
7. Add `/fast` parsing and idle command handling in `src/app.rs`, including optional `on`, `off`, and `status` forms if the implementation remains small.
8. Update `/status`, status-line rendering, autocomplete/help text, and model-switch status messages.
9. Update `README.md`, `docs/commands.md`, `docs/configuration.md`, `docs/providers.md`, `docs/workflows.md`, and `docs/glossary.md`.
10. Add focused tests and run `cargo fmt` plus `cargo test --locked --all-targets`.

## Tests

- `config.json` without `default_fast_mode` defaults to fast mode off.
- `default_fast_mode: true` loads and persists without losing existing config fields.
- `models.json` parses `fast_mode.supported`.
- Unsupported or missing fast-mode metadata produces `FastModeState { preferred: true, supported: false, active: false }`.
- A supported ChatGPT Codex model produces `active: true` when preference is on.
- `/fast` parses as a local command and rejects unexpected arguments unless explicit `on`/`off`/`status` forms are implemented.
- `/fast` cannot change preference during an active turn.
- Toggling `/fast` while on an unsupported provider stores the preference but reports unavailable.
- Switching from a supported Codex model to an unsupported model hides the active fast-mode signal without clearing the preference.
- Switching back to a supported Codex model restores active fast mode.
- `ChatGptCodexProvider` includes the verified fast-mode request field only when active.
- `OpenAiCompatibleProvider` request bodies are unchanged when fast-mode preference is on but unsupported.
- `/status` includes fast-mode state.
- README and bundled docs mention `/fast` and provider-specific support.
- `cargo fmt` passes.
- `cargo test --locked --all-targets` passes when practical.

## Documentation

- README command list and provider setup notes.
- `docs/commands.md` for `/fast` syntax and idle-only behavior.
- `docs/configuration.md` for `default_fast_mode` and `models.json` fast-mode capability metadata.
- `docs/providers.md` for the initial ChatGPT Codex-only support.
- `docs/workflows.md` for switching models/providers with fast-mode preference preserved.
- `docs/glossary.md` for "Fast mode" as a preference plus provider/model capability.

## Acceptance Criteria

- `/fast` toggles a persisted fast-mode preference.
- Fast mode is shown as enabled only when the active provider/model supports it.
- Unsupported providers/models do not show fast mode enabled and do not receive fast-mode request fields.
- ChatGPT Codex requests include the verified fast-mode option when the preference is on and the active Codex model supports it.
- Switching models/providers recomputes fast-mode active state without clearing the user's preference.
- `/status`, autocomplete/help, README, and bundled docs describe the feature accurately.
- `cargo fmt` and `cargo test --locked --all-targets` pass before implementation handoff.
