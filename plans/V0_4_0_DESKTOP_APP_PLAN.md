# v0.4.0 Desktop App Implementation Plan

## Goal

v0.4.0 adds a native desktop app for Cassady. A user can launch a windowed application that runs the real Cassady coding agent via the experimental Rust embedding API, with a chat composer, streaming assistant transcript, tool-call panels, approval dialogs, cancellation, and access-mode switching — all themed to match the `cassady-web/` landing page's terminal aesthetic.

Success statement:

> A user running the desktop app can start a chat, watch the assistant stream replies with tool calls, approve a shell command, cancel a running turn, and resume the same conversation later from the desktop app or from `cass --resume <id>` in a terminal. The window looks like the landing page: dark `#0b0d10` background, scanlines and film grain, IBM Plex Mono chrome, blue accent glow, and a terminal-style status footer.

## Scope

### In scope

- Add a new Cargo workspace member `cassady-desktop/` (Tauri 2 app) that depends on the `cassady` library via a path dependency.
- Port the `cassady-web/` landing-page design system (CSS tokens, IBM Plex fonts, scanlines/grain/vignette overlays, glow effects, terminal styling) into the desktop frontend.
- Build a chat surface: composer, streaming assistant transcript with full markdown rendering, tool-call blocks (`read`/`write`/`edit`/`shell`/`grep`/`ls`), approval dialogs, status footer, access-mode switcher, reasoning toggle, and turn cancellation.
- Build a Rust↔frontend streaming bridge over Tauri commands and `tauri::ipc::Channel` that drives `Session`/`Turn` and forwards `Event`s to the webview.
- Session lifecycle: new session, resume by id, list chats for a cwd, interoperable with `~/.cass/conversations/*.jsonl`.
- macOS-first build; Linux and Windows follow via Tauri's cross-build story.
- Documentation: this plan, a `ROADMAP.md` entry, a `cassady-desktop/README.md`, and a note in `docs/embedding.md` referencing the desktop app as a reference embedding.

### Out of scope

- Setup wizard GUI (provider/model/key selection inside the app). A "run `cass setup` first" card covers the no-config case for v0.4.0.
- Branch/restore GUI, config editor GUI, and auto-updater wiring.
- Mobile (Tauri mobile) and app-store signing/packaging pipelines.
- Custom tools or custom provider traits (the embedding API does not allow these — only the six built-in tools and OpenAI-compatible / ChatGPT Codex providers).
- Replacing or modifying the `cass`/`cassady` CLI binaries or the `src/` tree. The desktop app embeds purely through the public `cassady::prelude` API.

## Context and Current State

### Landing page (`cassady-web/`)

React 18 + Tailwind v4 (CSS `@theme`, no config file) + Framer Motion + Vite 6 + shadcn/ui (new-york). The full design system lives in `cassady-web/src/index.css`:

- Background tokens: `--color-bg #0b0d10`, `--color-bg-soft #111418`, `--color-bg-elev #161a1f`.
- Lines: `--color-line #232830`, `--color-line-soft #1a1f25`.
- Foreground: `--color-fg #c7ccd4`, `--color-fg-muted #8b929d`, `--color-fg-dim #5b626c`.
- Accent: `--color-accent #5b9fe0`, `--color-accent-glow rgba(91,159,224,0.18)`.
- Amber: `--color-amber #d4a55a`.
- Fonts: IBM Plex Mono (400/500/600) for chrome/code/headings, IBM Plex Sans (400/500) for body. Loaded via `@fontsource/ibm-plex-mono` and `@fontsource/ibm-plex-sans`.
- Signature effects: `.vignette::before` (radial dark corners), `.scanlines::before` (1px white lines every 3px at ~1.6% opacity, mix-blend overlay), `.grain::after` (SVG turbulence noise at 4% opacity), `.text-glow` (`text-shadow: 0 0 12px var(--color-accent-glow)`), `.terminal-shadow` (1px ring + deep drop + accent halo), `.grid-fade` (radial-masked 44px grid).
- Shadcn primitives in `cassady-web/src/components/ui/`: `button.tsx` (default/outline/ghost/link, sizes), `card.tsx`, `separator.tsx`, `tooltip.tsx`, `accordion.tsx`.
- The animated `TerminalDemo.tsx` is the visual reference for the chat surface: title bar with three gray dots, 380px body with internal scanlines and a blinking block cursor, dot-separated status footer whose color shifts (accent=running, amber=approval, dim=idle).

### Embedding API (`src/embedding.rs`, exported via `cassady::prelude`)

- `SessionBuilder::new()` with `.config_root()`, `.cwd()`, `.access_mode()`, `.model()`, `.base_url()`, `.api_key_env()`, `.reasoning_effort()`. Terminal methods: `.build()`/`.new_session()` and `.resume(id)`.
- `Session` exposes `id()`, `cwd()`, `model()`, `access_mode()`, `reasoning_effort()`, `conversation_path()`, `records()`, `resume_warning()`, `info() -> ConversationInfo`.
- `Session::start_turn(message)` consumes the session and returns a `Turn` (type-level guarantee against overlapping turns). `Turn::next_event() -> Option<Event>` streams events until `Event::Finished`. `Turn::finish()` recovers the `Session`; `Turn::cancel()` aborts and repairs the JSONL transcript. `Turn::approve(id)`/`deny(id)` resolve pending approvals.
- `Event` variants: `AssistantChunk`, `ReasoningChunk`, `ToolCallStarted { id, name, arguments }`, `ToolOutputChunk { id, name, stream, content }`, `ToolResult { id, name, ok, content }`, `ApprovalRequested(ApprovalRequest)`, `ApprovalResolved { request_id, approved }`, `Status(String)`, `Finished`.
- `ApprovalRequest` carries `request_id`, `name`, `arguments`, `reason`.
- Requires a Tokio runtime. Reuses `~/.cass` config, providers, prompts, security policy, tools, and JSONL storage. No custom tools or providers.

### Repo state

Single crate `cassady` (lib + bins `cass`/`cassady`). No existing desktop/GUI work. `AGENTS.md` documents release, npm, roadmap, and plan conventions. The root `Cargo.toml` is the `cassady` package; v0.4.0 adds a `[workspace]` block and a `cassady-desktop/` member without moving the existing crate.

### Tauri 2 integration points

- Vite frontend at `frontendDist: ../dist`, dev server at `http://localhost:5173` with `strictPort: true`.
- Commands are `#[tauri::command]`; `async` commands run on Tauri's async runtime (Tokio). State via `.manage(...)` and `tauri::State`.
- `tauri::ipc::Channel<T>` is the recommended streaming primitive — typed and ordered, better than `emit`/`listen` for token streams.

## Design Principles

1. **Fidelity to the landing page.** Reuse the exact CSS tokens, fonts, and effects by copying `cassady-web/src/index.css` and the IBM Plex `@fontsource` setup verbatim. The desktop chrome is a direct port, not a reinterpretation.
2. **Drive the real agent.** Embed `cassady` as a path dependency and call `SessionBuilder`/`Turn`/`Event` directly — never spawn the `cass` binary. The GUI is a peer of the TUI, reusing the same config, providers, tools, safety policy, and JSONL storage.
3. **Typed streaming.** Use `tauri::ipc::Channel<StreamEvent>` where `StreamEvent` is a serde-friendly mirror of `cassady::Event`. Channels are ordered and backpressure-friendly.
4. **One Tokio runtime.** Keep all `cassady` calls inside async commands on Tauri's async runtime; do not block the webview thread or spin a second runtime.
5. **No fork, no library changes.** Stay within the public embedding API. If a feature needs `Config`/`Conversation` internals, defer it rather than reaching into private modules.
6. **Workspace, not a new repo.** A Cargo workspace keeps one lockfile, shared `target/`, and path-dependency access to the lib.

## Design

### Repo layout

```
cassady/
├── Cargo.toml              # gains [workspace] members = ["cassady-desktop"]; stays the cassady package
├── src/                    # existing cassady lib + bins (untouched)
├── cassady-web/            # landing page (untouched; design source of truth)
└── cassady-desktop/        # NEW
    ├── Cargo.toml          # tauri deps + cassady = { path = ".." }
    ├── tauri.conf.json
    ├── build.rs            # tauri::build()
    ├── icons/               # app icons (reuse cass-logo-transparent.png)
    ├── capabilities/        # default.json (permissions)
    ├── src/
    │   ├── main.rs         # tauri entry (calls lib::run)
    │   ├── lib.rs          # tauri::Builder + invoke_handler + manage(state)
    │   ├── state.rs        # DesktopState { sessions, active_turns }
    │   ├── session.rs      # new_session / resume_session / list_chats / session_info commands
    │   ├── turn.rs         # start_turn (returns Channel<StreamEvent>), approve, deny, cancel_turn
    │   └── types.rs        # StreamEvent (mirror of cassady::Event), ConversationInfoDto, ChatSummary, etc.
    └── frontend/
        ├── package.json    # vite, react, @tauri-apps/api, @fontsource/ibm-plex-*, motion, react-markdown, tailwindcss v4
        ├── vite.config.ts  # port 5173 strict, watch ignore src-tauri
        ├── tsconfig.json
        ├── index.html      # <html class="dark">
        └── src/
            ├── main.tsx
            ├── App.tsx              # chat shell
            ├── index.css           # ported from cassady-web/src/index.css
            ├── lib/utils.ts        # cn() helper
            ├── lib/tauri.ts        # typed wrappers around invoke + Channel
            ├── hooks/useTurn.ts    # subscribes to Channel<StreamEvent>, buckets into transcript blocks
            ├── components/
            │   ├── ChatShell.tsx
            │   ├── Transcript.tsx
            │   ├── blocks/         # UserBlock, AssistantBlock, ReasoningBlock, ToolBlock, StatusBlock
            │   ├── Composer.tsx
            │   ├── ApprovalDialog.tsx
            │   ├── StatusFooter.tsx
            │   └── ui/             # button, card, separator, tooltip (copied from cassady-web)
            └── assets/            # cass-logo-transparent.png
```

The root `Cargo.toml` gains a `[workspace]` table:

```toml
[workspace]
members = ["cassady-desktop"]
resolver = "2"
```

The existing `cassady` package remains the root package (the root is implicitly a workspace member), so existing `cargo`/`cargo install`/release/npm scripts keep working unchanged.

### Design system port

Copy `cassady-web/src/index.css` verbatim into `cassady-desktop/frontend/src/index.css`. It defines the `@theme` token block, the global `* { border-color: var(--color-line) }` rule, scrollbars, `::selection`, and the `.vignette`/`.scanlines`/`.grain`/`.text-glow`/`.grid-fade`/`.terminal-shadow` utilities plus `@keyframes blink`. Apply `vignette scanlines grain` classes to the `ChatShell` root so the whole app window has the landing-page atmosphere. Load IBM Plex via `@fontsource/ibm-plex-mono` (400/500/600) and `@fontsource/ibm-plex-sans` (400/500) in `main.tsx`, identical to the landing page. Copy the shadcn `ui/` primitives (button, card, separator, tooltip, accordion) so buttons and cards match exactly. Reuse `cass-logo-transparent.png` for the app icon and in-app branding.

### Rust↔frontend bridge

**Commands** (all `async`, registered via `tauri::generate_handler!`):

| Command | Args | Returns | Notes |
|---|---|---|---|
| `new_session` | `{ cwd?, accessMode?, model?, baseUrl?, apiKeyEnv?, reasoningEffort? }` | `ConversationInfoDto` | Wraps `SessionBuilder::new_session`. Stores `Session` in `DesktopState` keyed by id. |
| `resume_session` | `{ chatId, cwd? }` | `ConversationInfoDto` | Wraps `SessionBuilder::resume`. Surfaces `resume_warning`. |
| `list_chats` | `{ cwd? }` | `Vec<ChatSummary>` | Reads `~/.cass/conversations/` via `config::cass_root()` + `conversations_dir()`. |
| `start_turn` | `{ message, onEvent: Channel<StreamEvent> }` | `TurnHandle` | Spawns a Tokio task that loops `turn.next_event()`, maps each `cassady::Event` → `StreamEvent`, sends over the channel. Stores the `Turn` in state. Returns a turn id. |
| `approve` | `{ turnId, requestId }` | `()` | Calls `turn.approve`. |
| `deny` | `{ turnId, requestId }` | `()` | Calls `turn.deny`. |
| `cancel_turn` | `{ turnId }` | `ConversationInfoDto` | Calls `turn.cancel().await`, recovers the `Session`, returns updated info. |
| `session_info` | `{ chatId }` | `ConversationInfoDto` | `Session::info()`. |

**State** (`DesktopState`, managed via `.manage()`):

- `Mutex<HashMap<String, Session>>` — sessions by chat id.
- `Mutex<HashMap<String, TurnEntry>>` — active turns by turn id. `TurnEntry` holds the `Turn` plus a handle to the worker task. Because `Session::start_turn` consumes the `Session`, the worker task holds it transiently and re-inserts the recovered `Session` into the sessions map on `finish`/`cancel`.

**StreamEvent** (`types.rs`, derives `Serialize`, camelCase for TS):

```rust
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum StreamEvent {
    AssistantChunk { text: String },
    ReasoningChunk { text: String },
    ToolCallStarted { id: String, name: String, arguments: serde_json::Value },
    ToolOutputChunk { id: String, name: String, stream: String, content: String },
    ToolResult { id: String, name: String, ok: bool, content: String },
    ApprovalRequested { requestId: String, name: String, arguments: serde_json::Value, reason: String },
    ApprovalResolved { requestId: String, approved: bool },
    Status { text: String },
    Finished,
    Error { message: String },
}
```

A dedicated worker task per turn: `loop { match turn.next_event().await { Ok(Some(ev)) => map & channel.send, Ok(None) => break, Err(e) => send Error } }`. On normal completion the task calls `turn.finish()` and re-inserts the recovered `Session`; on cancellation the `cancel_turn` command calls `turn.cancel()` directly (the worker observes channel closure).

### Frontend chat surface

`ChatShell.tsx` is a full-viewport flex column:

- **Top bar** (ported `Nav.tsx` style, minus the landing links): logo + "cassady" wordmark, right side shows model + access-mode pill + reasoning toggle.
- **Transcript** (center, flex-1, scrollable): renders `TranscriptBlock[]` produced by `useTurn`. Block types mirror the landing page's `TerminalDemo.tsx` styling:
  - `user` → `› you` in accent, content in fg-muted.
  - `assistant` → `cass` in accent with text-glow, markdown body rendered with `react-markdown` (headings, lists, links, fenced code blocks). Inline code in accent, fenced code in `bg-bg-soft`.
  - `reasoning` → collapsible, dim, off by default.
  - `tool` → `· {name}` in fg-dim, arguments as a `<pre>`; live `ToolOutputChunk`s stream into the body; final `ToolResult` shows `✓`/`✗` in accent/amber + summary. Mirrors `BlockView` in `TerminalDemo.tsx`.
  - `status` → `· {text}` in amber.
- **Approval dialog**: when `ApprovalRequested` arrives, show a modal (the landing-page card style) with tool name, arguments (pretty-printed JSON), reason, and Approve / Deny buttons. Calls `approve`/`deny` commands.
- **Composer** (bottom): a textarea (mono font, accent `›` prompt), `Enter` to send, `Shift-Enter`/`Ctrl-J`/`Ctrl-Enter` for newline, Send button becomes Stop while a turn runs (calls `cancel_turn`). Left side: access-mode cycle (`Shift-Tab`) and reasoning-effort cycle (`Tab`) as small mono pills.
- **Status footer** (ported from `TerminalDemo.tsx` footer): `cass · {mode} · {state} · {model} · {cwd} · {id} · reasoning:{effort}`. State color shifts: running=accent, approval=amber, idle=fg-dim.

`useTurn.ts` subscribes to the `Channel<StreamEvent>` via `@tauri-apps/api/core`, appends chunks to the right block (assistant text accumulates, tool output streams into the open tool block until `ToolResult` closes it), and exposes `{ blocks, isRunning, pendingApproval, cancel }`.

### Session lifecycle UX

- On launch: if `~/.cass` has no valid provider config, show a small card "Run `cass setup` first" with a copy-to-clipboard `cass setup` command (reuse `InstallCommand.tsx` styling). Full setup wizard is deferred.
- "New chat" and "Open chat…" (resume) in the top bar. Resume shows a list of `ChatSummary { id, cwd, recordCount, mtime }` filtered to the current cwd (matching `cass --resume` behavior).
- On turn finish, surface `session.id()` and a "resume in terminal" hint, mirroring the CLI's resume-command printout.

## Implementation Steps

1. **Write the plan and roadmap entry.** This document and `ROADMAP.md` `## v0.4.0 — Desktop App`.
2. **Scaffold the workspace.** Add `[workspace] members = ["cassady-desktop"]` to the root `Cargo.toml`. Create `cassady-desktop/` with `Cargo.toml` (tauri deps + `cassady` path dep), `tauri.conf.json`, `build.rs`, `capabilities/default.json`, `icons/`, and `frontend/` (package.json, vite.config.ts, tsconfig, index.html, src skeleton). Confirm `cargo build -p cassady-desktop` and `npm run tauri dev` reach a blank window.
3. **Port the design system.** Copy `index.css`, `lib/utils.ts`, `ui/` primitives, IBM Plex `@fontsource` imports, and `cass-logo-transparent.png` into `cassady-desktop/frontend/src/`. Confirm the window opens with the dark themed background and scanlines/grain/vignette overlays rendering.
4. **Build the Rust bridge.** Implement `DesktopState`, `StreamEvent`, `ConversationInfoDto`, `ChatSummary`, and the commands in `cassady-desktop/src/{lib,state,types,session,turn}.rs`. Wire `tauri::Builder::default().manage(...).invoke_handler(generate_handler![...])`. Confirm a `new_session` → `session_info` round-trip works from the devtools console.
5. **Wire `start_turn` streaming.** Spawn the per-turn worker on `tauri::async_runtime::spawn`, send `StreamEvent`s over the `Channel`, handle `finish`/`cancel` re-insertion of `Session`. Verify against a read-only session against the current repo (e.g. "list the files in this directory") — assistant text and tool blocks should stream live.
6. **Build the chat UI.** Implement `ChatShell`, `Transcript`, the block components (`UserBlock`, `AssistantBlock` with `react-markdown`, `ReasoningBlock`, `ToolBlock`, `StatusBlock`), `Composer`, `StatusFooter`, and `ApprovalDialog` using the ported design tokens. Wire `useTurn` to the channel.
7. **Add approvals + cancellation.** `ApprovalDialog` → `approve`/`deny`; Stop button → `cancel_turn`. Test in `workspace-edit` mode (shell command triggers an approval) and `full-access` with `confirm_destructive_operations` (write/edit triggers an approval).
8. **Session management.** `list_chats` + resume UI; new-chat; interop check: start a chat in the desktop app, resume it with `cass --resume <id>` in a terminal, and vice versa.
9. **Polish.** Scroll-to-bottom behavior, keyboard shortcuts matching the TUI where sensible (`Esc` to cancel, `Shift-Tab`/`Tab` for mode/reasoning), window title showing chat id + cwd.
10. **Build + verify.** `cargo build -p cassady-desktop`, `cargo test --locked --all-targets` (existing tests must still pass; add a small Rust test for `StreamEvent` mapping and session round-trip against a temp `config_root`), and `cd cassady-desktop/frontend && npm run build && tsc --noEmit`.

## Tests

- **Rust:** unit test that maps each `cassady::Event` variant → `StreamEvent` and asserts the JSON shape. Integration test using `SessionBuilder::new().config_root(tempdir).cwd(tempdir).access_mode(ReadOnly).build()` + a stub provider (or skip if no key in CI) to confirm `new_session`/`start_turn`/`cancel` don't panic. Existing `cargo test --locked --all-targets` must stay green.
- **Frontend:** `tsc --noEmit` typecheck. Manual smoke-test checklist documented in `cassady-desktop/README.md`: new chat, streaming text, tool block streaming, approval flow, cancel, resume interop.
- **Manual:** run the desktop app against this repo, run `cass --resume <id>` on a chat started in the GUI, and run the GUI on a chat started in the CLI.

## Documentation

- `plans/V0_4_0_DESKTOP_APP_PLAN.md` (this document).
- `ROADMAP.md` `## v0.4.0 — Desktop App` entry with areas for Workspace Setup, Design System Port, Chat Surface, Session Lifecycle, and Verification.
- `cassady-desktop/README.md`: prerequisites (Tauri system deps), `npm run tauri dev`, `npm run tauri build`, and the "run `cass setup` first" requirement.
- `docs/embedding.md`: a short note pointing to the desktop app as a reference embedding of the library API.

## Acceptance Criteria

- `cassady-desktop/` builds with `cargo build -p cassady-desktop` and `npm run tauri build` produces a runnable macOS app.
- Launching the app, starting a chat, streaming an assistant reply with tool calls, approving a shell command, and cancelling a running turn all work end-to-end.
- A chat started in the desktop app is resumable from `cass --resume <id>`, and a chat started in the CLI is openable in the desktop app.
- The UI matches the landing-page theme: same color tokens, IBM Plex fonts, scanlines/grain/vignette overlays, accent glow, terminal-style status footer.
- `cargo test --locked --all-targets` passes across the workspace; `cargo fmt` is clean.
- No changes to `src/` — the desktop app embeds purely through the public `cassady::prelude` API.
