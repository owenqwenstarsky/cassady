# v0.2.9 Provider Login Management Implementation Plan

## Goal

This release focuses on making provider configuration available from both the shell and an active Cassady chat. Users should be able to run `cass login` or type `/login` to configure one or more OpenAI-compatible providers, and use `cass logout` or `/logout` to remove saved providers and their model entries without hand-editing JSON files.

Success statement:

> A user can add, switch, and remove provider/model configuration from Cassady's normal command surfaces, then continue chatting with a valid active provider.

## Scope

### In scope

- Add `cass login` as an alias-style command for the existing setup wizard.
- Add `/login` inside the TUI, available only while idle.
- Add `cass logout` with an interactive provider removal menu.
- Add `/logout` inside the TUI, available only while idle.
- Remove provider definitions and their associated `models.json` entries together.
- Update `config.json` active defaults after removal so they do not point at missing providers or models.
- Reload active config after login/logout inside the TUI.
- Document the new commands in bundled command/config docs.
- Add focused unit tests for provider removal and local command parsing/autocomplete.

### Out of scope

- Browser OAuth or provider-hosted account login flows.
- Storing literal API keys from the wizard by default.
- Non-OpenAI-compatible provider protocols.
- Deleting shell environment variables or secrets outside `~/.cass`.
- Publishing, tagging, or preparing release artifacts.

## Context

Cassady already has most provider setup primitives:

- `src/setup.rs` contains the interactive provider/model setup wizard, provider catalog, model discovery, and JSON upsert helpers.
- `src/config.rs` owns `config.json`, `providers.json`, `models.json`, active provider/model resolution, and validation.
- `src/app.rs` owns top-level CLI dispatch plus in-chat slash command parsing and execution.
- `docs/commands.md`, `docs/configuration.md`, and `docs/workflows.md` document the existing `cass setup`, `cass check`, and `/model` behavior.

The existing setup wizard writes provider connection definitions to `providers.json`, model metadata to `models.json`, and active defaults to `config.json`. The new login command can reuse that flow. Logout needs a new inverse operation that edits all three files consistently.

## Design Principles

1. Reuse setup behavior instead of creating a second provider configuration path.
2. Keep removal explicit and reversible by avoiding broad file deletion and by preserving unrelated providers/models.
3. Never remove API keys from the user's shell or keychain; Cassady only edits its own config files.
4. Keep in-chat provider management idle-only, because active turns depend on a stable provider config.

## Design

### CLI commands

Add these subcommands:

```text
cass login
cass logout
```

`cass login` runs the same interactive wizard as `cass setup`, with text that frames the action as adding or updating provider login configuration. It may start a chat afterward when invoked from an otherwise normal chat startup path only if the existing setup outcome says it should; direct `cass login` should save config and exit.

`cass logout` opens a multi-select menu of saved providers:

```text
Remove saved providers

  [ ] OpenAI      openai · gpt-4.1
  [ ] Groq        groq · llama-3.3-70b-versatile
```

After confirmation, Cassady removes the selected providers from `providers.json` and removes `models.json` entries whose `provider` matches a removed provider id. If the active provider was removed, Cassady selects the first remaining provider and one of its models. If no providers remain, `default_provider`, `default_model`, and `default_reasoning_effort` are cleared so the next `cass` run offers setup.

### In-chat commands

Add slash commands:

```text
/login
/logout
```

Both commands are idle-only. Because the TUI uses the alternate screen and raw input mode, command execution should temporarily leave the TUI, run the existing menu-driven flow in the normal terminal, reload config, then re-enter the TUI and append a status block.

After `/login`, reload `Config` from disk and keep the current conversation open. If the active provider/model changed, future turns use the new provider and model. The status block should show the active provider and model.

After `/logout`, reload `Config` when a provider remains. If no provider remains or config cannot resolve, keep the chat open but append a clear status/error telling the user to run `/login` before sending another turn.

### Provider/model removal helper

Add reusable setup/config helpers:

- `configured_providers(root) -> Vec<ProviderLogoutCandidate>`
- `remove_providers(root, provider_ids) -> LogoutResult`

`LogoutResult` should include removed provider ids, removed model count, remaining provider count, and the new active provider/model when one exists. This keeps CLI output, TUI status, and tests deterministic.

## Implementation Steps

1. Add the v0.2.9 roadmap entry and this plan.
2. Add `Login` and `Logout` CLI variants and dispatch them from `app::run`.
3. Refactor setup mode text as needed so `cass login` can share the setup wizard.
4. Implement provider removal helpers in `src/setup.rs` using existing config structs.
5. Add menu-driven `setup::logout(root)` for CLI and TUI use.
6. Add `/login` and `/logout` to local command parsing, autocomplete, and idle command handling.
7. Add a small terminal leave/re-enter helper around blocking login/logout menus inside the TUI.
8. Update command/config/workflow docs.
9. Add focused tests for removal behavior and command parsing/autocomplete.

## Tests

- Removing one provider preserves unrelated providers and models.
- Removing the active provider chooses a valid remaining provider/model.
- Removing all providers clears active defaults.
- Removing an unknown provider id is rejected.
- `/login` and `/logout` parse only with no arguments.
- Command autocomplete lists `/login` and `/logout`.
- `cargo fmt` passes.
- `cargo test --locked --all-targets` passes when practical.

## Documentation

- Update `docs/commands.md` with `cass login`, `cass logout`, `/login`, and `/logout`.
- Update `docs/configuration.md` to point users toward login/logout for managed provider edits.
- Update `docs/workflows.md` with login/logout examples near model/provider workflows.

## Acceptance Criteria

- `cass login` opens the provider setup wizard and exits after saving direct login changes.
- `cass logout` removes selected providers and their models with confirmation.
- `/login` and `/logout` work from an idle chat and reload active provider config afterward.
- Removing the active provider never leaves `config.json` pointing at a missing provider/model.
- Existing `cass setup`, first-run setup, `cass check`, and `/model` behavior still work.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
