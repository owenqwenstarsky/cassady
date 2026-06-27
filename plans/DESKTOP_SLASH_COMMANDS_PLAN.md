# Desktop Slash Commands Implementation Plan

## Goal

Full, clean slash-command support in the Cassady desktop app, parity with the CLI's 8 in-chat commands (`/branch` `/fast` `/login` `/logout` `/model` `/new` `/resume` `/status`), with a filterable autocomplete popup in the composer and auto-run on full selection. The command **registry, parser, autocomplete, and execution** move out of `src/app.rs` into a new shared `cassady::commands` core module so the CLI TUI and the Tauri desktop (and any future host) share one command system.

## Scope

### In scope

- New `cassady::commands` module: catalog (`COMMANDS`/`CommandSpec`), `LocalCommand`/`FastModeCommand` enums, `parse`, autocomplete builders (`build_autofill` + `command_autofill`/`model_autofill`/`resume_chat_autofill`), and pure action functions. Move `AutoFillItem`/`AutoFillMenu` here (add `Serialize`/`Deserialize`), out of `ui/`.
- A `CommandOutcome` enum + `commands::execute(input, &mut CommandContext)` single entry point that both hosts use. Interactive commands return `OpenBranchPicker`/`OpenLoginWizard`/`OpenLogoutPicker` so each host renders its own picker and calls back into core for the apply step.
- Refactor `src/app.rs` dispatcher to call `commands::*` instead of its private inline copies; delete the duplicated private functions. CLI user-facing behavior unchanged.
- `embedding::Session`: add `config_mut()`/`set_conversation()` accessors; refactor `Session::set_model` to delegate to `commands::apply_model` (dedup).
- Desktop Tauri commands: `list_slash_commands`, `slash_autofill`, `run_slash_command` (calls `commands::execute`), plus `create_branch_from_checkpoint`, `apply_provider_login`, `discover_models`, `list_logout_candidates`, `remove_providers` for the modal apply steps.
- Frontend `SlashMenu` popup on the `Composer` textarea (reuse the `ModelSelector` pattern). Auto-run complete commands on selection and on manual Enter.
- Frontend modals: `BranchModal` (branch/restore picker), `LoginModal` (provider setup wizard), `LogoutModal` (provider removal). Reuse `ApprovalDialog`/`Card` styling.
- Update `docs/commands.md` and `ROADMAP.md`.

### Out of scope

- Adding new commands beyond the 8 (no `/help`, `/clear`, `/compact`, `/exit`).
- DMG/installer or Tauri auto-update changes.
- Cross-branch file-restore behavior changes (still uses conservative `file_edits` planning).
- Refactoring the CLI setup wizard itself (`setup::run` stays crossterm-based; desktop gets its own React wizard).

## Context or Current State

- **Desktop** (`cassady-desktop/`): React 18 + Vite + Tailwind v4 + Tauri 2, dark terminal aesthetic. `Composer.tsx` is a plain auto-growing textarea (Enter sends, Esc cancels); no autocomplete/slash UI. `ModelSelector.tsx` is the only existing filterable dropdown — a reusable pattern. IPC centralized in `src/lib/tauri.ts`; Tauri commands registered in `src-tauri/src/lib.rs:12-25`. `embedding::Session` is the runtime; `Session.config` is private.
- **CLI** (`src/app.rs`): 8 slash commands, private `LocalCommand`/`CommandSpec`/`COMMANDS`/`parse_local_command`/`build_autofill`/`*_autofill`/`apply_*`/`chat_status` etc. inlined (lines ~1925-2410, dispatcher 702-1064). Autocomplete popup via `ui/autofill.rs` (`AutoFillMenu`/`AutoFillItem` — pure data, no ratatui).
- **Core already public & UI-agnostic** (no extraction needed, just facades in `commands`): `config::save_fast_mode_preference`, `Config::fast_mode_state`, `config::load_or_create_default_*_registry`, `config::save_last_used_provider`, `setup::apply_setups`/`configured_providers`/`remove_providers`/`provider_catalog`/`discover_models`, `conversation::Conversation::{create,load}`/`list_chats`, `branch::load_family`/`create_branch`/`checkpoint_records`/`checkpoint_title`, `file_edits::plan_restore`/`apply_restore_plan`/`summarize_plan`.
- **Gaps to fill in core**: lift the private `apply_fast_mode_command`, `apply_model_selection`, `create_new_conversation`, `chat_status`, `fast_mode_status`, `model_status_message`, and the autocomplete builders into `commands`; add a UI-agnostic branch orchestrator (`create_branch` + optional file restore) and a `CommandOutcome`/`execute` dispatch.

## Design Principles

1. **Single source of truth.** Catalog, parser, autocomplete, and pure actions live in `cassady::commands`; neither `app.rs` nor the desktop duplicates them.
2. **Core does data, host does picker.** Interactive commands (`/login` `/logout` `/branch`) return an "open picker" outcome + candidate data; the host renders its own UI and calls back into core for the apply. Pure commands (`/fast` `/model` `/new` `/resume` `/status`) execute fully in core and return a displayable result.
3. **No behavior change for the CLI.** The TUI refactor is mechanical: replace inline private calls with `commands::*` calls; identical user-facing behavior.
4. **Reuse existing desktop wiring.** `/new`//`/resume`//`/model` reuse the existing `new_session`/`resume_session`/`update_session_settings` Tauri commands and frontend handlers where possible; only `/fast`, `/status`, `/branch`, `/login`, `/logout` need new Tauri commands.
5. **Auto-run on full selection.** Selecting a complete command (no-arg, or a specific model/chat arg) runs it immediately and clears the input; selecting a value-command name inserts it and keeps the menu open for arg selection; manual Enter on a complete typed command also runs.

## Design

### `cassady::commands` module (`src/commands.rs`)

```rust
pub struct CommandSpec { name, usage, description, takes_value }   // moved from app.rs
pub const COMMANDS: &[CommandSpec];                                 // moved
pub enum LocalCommand { Branch, Fast(FastModeCommand), Login, Logout, Model(String), New, Resume(String), Status }
pub enum FastModeCommand { Toggle, On, Off, Status }
pub fn parse(input: &str) -> Result<LocalCommand, String>;          // = parse_local_command

pub struct AutoFillItem { label, insert, detail }                   // moved from ui/autofill.rs + Serialize
pub struct AutoFillMenu { title, replacement_start, replacement_end, items, selected }  // + Serialize
pub fn build_autofill(input, selected, &Config, cwd) -> Result<Option<AutoFillMenu>>;   // moved
// + command_autofill, model_autofill, resume_chat_autofill, model_matches, model_detail, chat_matches, chat_detail, short_model_name

pub struct CommandContext<'a> { config: &'a mut Config, conversation: &'a Conversation, cwd: &'a Path, busy: bool, status: &'a str }
pub enum CommandOutcome {
    Status(String),                 // /fast, /status, /model success
    NewChat { conversation: Conversation, status: String },          // /new
    ResumedChat { conversation: Conversation, warning: Option<String>, status: String }, // /resume
    OpenBranchPicker { family: BranchFamily },                       // /branch
    OpenLoginWizard,                                                // /login
    OpenLogoutPicker { candidates: Vec<ProviderLogoutCandidate> },   // /logout
    Busy(String), Error(String),
}
pub fn execute(input: &str, ctx: &mut CommandContext) -> CommandOutcome;
```

Pure action functions (called by `execute` and reusable directly): `apply_fast_mode(&mut Config, FastModeCommand) -> Result<FastModeOutcome>`, `apply_model(&mut Config, &str) -> Result<ModelOutcome>` (resolve + switch provider + `save_last_used_provider`), `create_new(&Config, &Path) -> Result<Conversation>`, `load_for_resume(&Config, &str) -> Result<(Conversation, Option<String>)>`, `status_summary(...) -> StatusSummary`, `branch_family(&Config, &Conversation) -> Result<BranchFamily>`, `create_branch(&Config, &Conversation, &Checkpoint, restore_files: bool) -> Result<BranchOutcome>` (orchestrates `branch::create_branch` + optional `file_edits::plan_restore`/`apply_restore_plan`), `logout_candidates(root) -> Result<Vec<ProviderLogoutCandidate>>`, `remove_providers(root, &[String]) -> Result<LogoutResult>`, `login_catalog() -> Vec<ProviderCatalogEntry>`, `apply_login(root, &[SetupSelection], usize) -> Result<()>`, `discover_models(base_url, api_key) -> Result<Vec<String>>` (thin facades over `setup::*`).

### `embedding::Session` changes

- `pub fn config_mut(&mut self) -> &mut Config` and `pub fn set_conversation(&mut self, Conversation)`.
- `Session::set_model` refactored to delegate to `commands::apply_model` (removes the duplicated resolve/switch logic in `embedding.rs:241`).

### CLI refactor (`src/app.rs`)

- Delete the private command helpers (lines ~1925-2410) and the inline dispatcher logic (702-1064); replace with `commands::parse` + `commands::execute` + a small `match CommandOutcome { ... }` that maps to TUI state mutations (`transcript.push`, `status =`, `branch_menu = Some(...)`, `run_login_menu_from_tui`/`run_logout_menu_from_tui` for the wizard outcomes, `stick_to_bottom`/`scroll` recompute). Net TUI behavior unchanged.

### Desktop Tauri commands (`src-tauri/src/session.rs` + `lib.rs`)

- `list_slash_commands() -> Vec<CommandSpecDto>` (from `commands::COMMANDS`).
- `slash_autofill(input, cwd) -> Option<AutoFillMenuDto>` (from `commands::build_autofill`).
- `run_slash_command(input) -> CommandOutcomeDto` (builds `CommandContext` from the `Session` via the new accessors, calls `commands::execute`). Maps `NewChat`/`ResumedChat` to `session.set_conversation(...)`; maps `Open*Picker` to DTOs the frontend renders.
- `create_branch_from_checkpoint(chat_id, checkpoint_id, restore_files) -> BranchResultDto`.
- `apply_provider_login(selections, active_index) -> ()` and `discover_models(base_url, api_key) -> Vec<String>`.
- `list_logout_candidates() -> Vec<ProviderLogoutCandidateDto>` and `remove_providers(ids) -> LogoutResultDto`.
- `/fast` handled inside `run_slash_command` via `commands::apply_fast_mode` + `session.config_mut()`; `/status` via `commands::status_summary` + `session_info`.

### Frontend (`cassady-desktop/src/`)

- `lib/tauri.ts`: add wrappers for the new commands + `CommandOutcome`/`CommandSpec`/`AutoFillMenu` TS mirrors.
- `components/SlashMenu.tsx`: popup anchored to the composer textarea, triggered when `value.startsWith('/')` and no newline before cursor. Renders `command_autofill` list (usage + description) or the arg list (models/chats) from `slash_autofill`. Arrow keys + Enter + Escape + click-outside (mirror `ModelSelector`). On select-complete -> call `runSlashCommand` -> route outcome. On select-value-command-name -> insert and keep open.
- `Composer.tsx`: wire `SlashMenu` (passes value, cursor, running); intercept arrow/Enter/Esc when menu open; auto-run complete commands; keep existing Enter-to-send for normal messages.
- `components/BranchModal.tsx`: lists `BranchFamily` (branches + checkpoints), checkpoint select, conversation-only vs +files-restore toggle, preview via `summarize_plan`, confirm -> `create_branch_from_checkpoint`. Styled like `ApprovalDialog`.
- `components/LoginModal.tsx`: multi-step wizard — provider catalog -> base url / api-key-env / model id (with `discover_models` lookup) -> apply. Largest UI piece.
- `components/LogoutModal.tsx`: candidate list with checkboxes + confirm -> `remove_providers`.
- `ChatShell.tsx`: hold modal state (`branchFamily`, `loginOpen`, `logoutCandidates`) driven by `runSlashCommand` outcomes; render the three modals.
- `Transcript`/blocks: reuse `StatusBlock` for `/fast`//`/status`//`/model`//`/new`//`/resume` status messages; an error block for errors; a small status block for `ResumedChat` warning.

## Implementation Steps

1. Create `src/commands.rs`: move `AutoFillItem`/`AutoFillMenu` from `ui/autofill.rs` (add serde derives), move catalog/parser/autocomplete/pure-action functions from `app.rs`, add `CommandOutcome`/`CommandContext`/`execute` + the thin `setup`/`branch`/`file_edits` facades. Register `pub mod commands;` in `lib.rs`.
2. Add `embedding::Session::config_mut`/`set_conversation`; refactor `Session::set_model` to delegate to `commands::apply_model`.
3. Refactor `src/app.rs` dispatcher to use `commands::execute` + outcome match; delete the old private helpers. Run `cargo test` — CLI behavior must be unchanged.
4. Add desktop Tauri commands + DTOs in `src-tauri/src/session.rs` and register in `lib.rs`. Add TS mirrors + wrappers in `lib/tauri.ts`.
5. Build `SlashMenu.tsx`; wire into `Composer.tsx` (trigger, keys, auto-run, manual Enter parse).
6. Wire `/fast`, `/status`, `/model`, `/new`, `/resume` outcomes to existing frontend handlers / status blocks. Verify end-to-end.
7. Build `BranchModal.tsx` + `create_branch_from_checkpoint` wiring.
8. Build `LogoutModal.tsx` + removal wiring.
9. Build `LoginModal.tsx` + `apply_provider_login`/`discover_models` wiring.
10. Update `docs/commands.md` (note desktop support) and `ROADMAP.md`. Run `cargo test --locked --all-targets` and the desktop build (`cd cassady-desktop && npm run build` + `cargo build`).

## Tests

- Unit (Rust): `commands::parse` (all 8 + aliases + usage errors), `apply_fast_mode` (toggle/on/off/status + unsupported model), `apply_model` (same-provider switch + cross-provider switch + unknown model), `status_summary`, `build_autofill`/`command_autofill`/`model_autofill`/`resume_chat_autofill` (matching, suppression on exact match, replacement ranges), `create_branch` orchestration (with/without file restore).
- Integration (Rust): extend `tests/` to cover `commands::execute` outcomes for each `LocalCommand` variant using a temp `~/.cass`.
- Desktop Tauri: smoke tests for `run_slash_command` + `slash_autofill` + `create_branch_from_checkpoint` + `apply_provider_login` + `remove_providers`.
- Manual: each of the 8 commands via the `SlashMenu` (autocomplete + manual typing + auto-run + busy guard); each modal flow; CLI parity check.

## Documentation

- `docs/commands.md`: add a note that all 8 in-chat commands work in the desktop app via the composer slash menu.
- `ROADMAP.md`: add an entry under the next release referencing this plan.

## Acceptance Criteria

- All 8 slash commands work in the desktop app with autocomplete and auto-run, behaviorally parallel to the CLI.
- `cassady::commands` is the single source for the catalog, parser, autocomplete, and pure actions; `src/app.rs` contains no private duplicates; the CLI's user-facing behavior is unchanged.
- `embedding::Session` exposes the new accessors and `set_model` delegates to `commands::apply_model`.
- The three modals (Branch, Login, Logout) render cleanly in the existing dark terminal aesthetic and round-trip through the core facades.
- `cargo test --locked --all-targets` passes; `cd cassady-desktop && npm run build` and `cargo build` pass.
