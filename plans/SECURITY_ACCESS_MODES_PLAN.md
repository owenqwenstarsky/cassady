# Security Policy and Workspace Edit Mode Implementation Plan

## Goal

Implement a new access mode that lets the agent inspect and modify files inside the launch workspace without per-file confirmation, while requiring explicit user confirmation before any shell command executes. At the same time, refactor Cass's security and safety model so future restrictions can be added centrally and safely instead of scattering `AccessMode` checks throughout tools, path helpers, prompts, and UI code.

Working name for the new mode: `workspace-edit`.

## Desired user-facing behavior

Cass should support three access modes:

| Mode | Read/list/search | Write/edit | Shell |
| --- | --- | --- | --- |
| `read-only` | Allowed only inside workspace and bundled Cass docs | Denied | Denied |
| `workspace-edit` | Allowed only inside workspace and bundled Cass docs | Allowed inside workspace only; bundled Cass docs remain read-only | Requires confirmation before execution |
| `full-access` | Allowed using normal OS permissions, except policy-specific blocked roots | Allowed using normal OS permissions, except policy-specific blocked roots | Allowed without confirmation by default |

Notes:

- The launch workspace is the process cwd or the path supplied by `--cwd`.
- Bundled Cass docs remain readable but never writable in all modes.
- `workspace-edit` should be safe enough to use as a normal coding mode.
- Shell approval must happen before process spawn; denied shell calls should not execute at all.
- If a tool call is denied or rejected by the user, Cass should append a normal failed tool result so provider tool-call structure remains valid.

## Design principles

1. Security decisions must be enforced in Rust code, not only in the system prompt.
2. All mode and path authorization should go through one central policy layer.
3. Tools should perform domain work, not own access-control logic.
4. UI confirmation should be a generic approval mechanism, not hardcoded inside the shell tool.
5. The policy model should support future rules such as destructive write confirmation, protected path deny-lists, file-size limits, env-file confirmation, and shell command classification.
6. Path checks must be symlink-aware and should avoid time-of-check/time-of-use gaps where practical.

## High-level architecture

Add a new security/policy layer that sits between the agent loop and tool execution.

```text
model tool call
      |
      v
agent builds ToolRequest / ToolAction
      |
      v
SecurityPolicy::check(...)
      |
      +-- Allow --------> execute tool
      |
      +-- Ask ----------> emit ApprovalRequested, wait for UI decision
      |                    + approve -> execute tool
      |                    + deny ----> failed tool result
      |
      +-- Deny ---------> failed tool result
```

The tool implementations should still validate their inputs and operate defensively, but they should rely on pre-resolved policy decisions rather than checking `mode.can_write()` directly.

## Proposed modules and types

### `src/access.rs`

Extend `AccessMode`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessMode {
    #[default]
    ReadOnly,
    WorkspaceEdit,
    FullAccess,
}
```

Replace the binary `toggle()` with a cycle-friendly method:

```rust
impl AccessMode {
    pub fn next(self) -> Self {
        match self {
            Self::ReadOnly => Self::WorkspaceEdit,
            Self::WorkspaceEdit => Self::FullAccess,
            Self::FullAccess => Self::ReadOnly,
        }
    }

    pub fn as_str(self) -> &'static str { ... }
}
```

Avoid adding broader helpers like `can_write()` unless they delegate to the new policy layer or are clearly display-only. The current `can_write()` encourages bypassing the centralized policy.

### New `src/security.rs`

Add a policy module with explicit action and decision types.

```rust
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub mode: AccessMode,
    pub cwd: PathBuf,
    pub workspace_root: PathBuf,
    pub docs_root: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub blocked_write_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ToolAction {
    List { path: PathBuf },
    Read { path: PathBuf },
    Search { path: PathBuf },
    Write { path: PathBuf, creates: bool, overwrites: bool },
    Edit { path: PathBuf },
    Shell { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Ask { reason: String },
    Deny { reason: String },
}

pub struct SecurityPolicy;
```

Suggested public methods:

```rust
impl SecurityPolicy {
    pub fn check(ctx: &SecurityContext, action: &ToolAction) -> PolicyDecision;
    pub fn tool_availability(ctx: &SecurityContext, tool: &str) -> ToolAvailability;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAvailability {
    Unavailable,
    Available,
    RequiresApproval,
}
```

`ToolAvailability` is used for tool spec exposure and prompt text; `PolicyDecision` is used for specific tool calls.

### Path policy helpers

Move authorization-oriented path logic out of `src/tools/path.rs` into the policy layer or into a lower-level `security::paths` submodule.

Needed helpers:

```rust
pub fn canonicalize_existing(path: &Path) -> Result<PathBuf>;
pub fn canonicalize_for_create_or_write(path: &Path) -> Result<PathBuf>;
pub fn normalize_lexical(path: &Path) -> PathBuf;
pub fn is_under_root(path: &Path, root: &Path) -> bool;
pub fn is_under_any_root(path: &Path, roots: &[PathBuf]) -> bool;
```

Important details:

- Existing read/list/search paths should be fully canonicalized before root checks.
- Write paths may not exist, so canonicalize the nearest existing ancestor and append missing components.
- The workspace root, docs root, and blocked write roots should be canonicalized once when the tool/security context is built.
- For writes, if the final path exists, canonicalize it and confirm it is within the allowed root for `workspace-edit`.
- For writes to new paths, canonicalize the nearest existing parent and ensure that parent is within the workspace root and not under a blocked write root.
- Reject paths with no existing ancestor.

## Mode-specific policy rules

### `read-only`

- `ls`, `read`, and `grep` are available.
- Read/search/list paths must be under `workspace_root` or `docs_root`.
- `write`, `edit`, and `shell` are unavailable and denied.

### `workspace-edit`

- `ls`, `read`, and `grep` are available.
- Read/search/list paths must be under `workspace_root` or `docs_root`.
- `write` and `edit` are available for paths under `workspace_root` only.
- `write` and `edit` are denied under `docs_root` or any blocked write root.
- `write` and `edit` outside `workspace_root` are denied.
- `shell` is available but every shell command returns `Ask` before execution.

### `full-access`

- All tools are available.
- `ls`, `read`, and `grep` use normal OS permissions and are not restricted to workspace by default.
- `write` and `edit` use normal OS permissions but remain denied under `docs_root` and other blocked write roots.
- `shell` is allowed by default.
- Future config may add confirmation for destructive operations in this mode without changing tool implementations.

## Tool API changes

### `src/tools/mod.rs`

Replace mode-based availability with policy-based availability:

```rust
pub fn available_tool_names(ctx: &SecurityContext) -> Vec<String>;
pub fn specs(ctx: &SecurityContext) -> Vec<ToolSpec>;
```

`workspace-edit` should include `shell` in the tool specs because the model may request shell commands, but the prompt/tool description must state that shell requires user approval.

Update `ToolContext` so it includes a `SecurityContext` or has enough fields to construct one without duplicating policy data.

```rust
pub struct ToolContext {
    pub security: SecurityContext,
    pub model_result_limit: usize,
    pub runtime_tx: Option<mpsc::UnboundedSender<ToolRuntimeEvent>>,
}
```

If a smaller migration is desired, keep existing fields temporarily but add `security` and gradually remove direct `mode`, `read_roots`, and `blocked_write_roots` dependencies.

### Individual tools

Refactor tools so they no longer call `ctx.mode.can_write()`.

- `ls`, `read`, `grep`: resolve target path, then rely on prior or local policy validation for list/read/search action.
- `write`, `edit`: resolve target path through the policy-aware resolver and do not contain mode-specific checks.
- `shell`: remove `if !ctx.mode.can_write()` and trust that the agent/policy gate checked the command before calling `shell::run`.

To avoid accidental direct tool execution bypasses, `tools::execute` should remain a second enforcement point:

1. Convert the tool name/args to a `ToolAction`.
2. Check policy.
3. Only execute on `Allow`.
4. Return a failed `ToolOutput` for `Deny` or unexpected `Ask` when no approval has been provided.

The agent should normally perform this check before approval, but `tools::execute` should defensively enforce it too.

## Approval flow

### Agent event changes

Extend `AgentEvent`:

```rust
pub enum AgentEvent {
    ...
    ApprovalRequested {
        request_id: String,
        tool_call_id: String,
        name: String,
        arguments: Value,
        reason: String,
    },
    ApprovalResolved {
        request_id: String,
        approved: bool,
    },
}
```

Add an input channel from UI to agent:

```rust
#[derive(Debug, Clone)]
pub enum AgentCommand {
    ApprovalDecision {
        request_id: String,
        approved: bool,
    },
}
```

Update `AgentSettings` or `run_turn` signature to accept the receiver:

```rust
pub async fn run_turn(
    conversation: Conversation,
    user_message: String,
    settings: AgentSettings,
    tx: mpsc::UnboundedSender<AgentEvent>,
    command_rx: mpsc::UnboundedReceiver<AgentCommand>,
) -> Result<Conversation>
```

Alternative: pass a single-use approval responder through a dedicated channel stored in `AgentSettings`. The explicit command channel is more extensible for future interactive controls.

### Agent execution logic

Before executing each tool call:

1. Emit `ToolCallStarted` as today.
2. Build `ToolAction` from the tool call.
3. Call `SecurityPolicy::check`.
4. If `Allow`, execute the tool.
5. If `Deny`, create `ToolOutput { ok: false, content: reason }` without executing.
6. If `Ask`, emit `ApprovalRequested` and wait for matching `AgentCommand::ApprovalDecision`.
   - Approved: execute the tool.
   - Denied: create failed tool output like `user denied shell command: <reason>`.
   - Turn cancelled while waiting: abort cleanly through existing cancellation path.

Approval wait should be cancellable when the turn task is aborted. Since the agent currently runs inside a Tokio task and cancellation aborts the task, this is mostly automatic, but avoid spawning detached execution before approval.

### UI behavior in `src/app.rs`

Add pending approval state:

```rust
struct PendingApproval {
    request_id: String,
    tool_call_id: String,
    name: String,
    arguments: Value,
    reason: String,
}
```

When `ApprovalRequested` arrives:

- Store it as pending.
- Add or update a transcript/status block showing the request.
- Set status to `approval required: press y to approve, n to deny`.

Key handling while pending approval:

- `y` or `Y`: send `AgentCommand::ApprovalDecision { approved: true }`.
- `n`, `N`, or `Esc`: send denied decision.
- `Ctrl-C`: preserve existing turn cancellation behavior.
- Other text input should either be ignored with a status hint or continue editing the input box but not submitted until approval resolves. Simpler first version: ignore non-approval keys except cancellation and scrolling.

Render the approval content clearly, especially for shell:

```text
Shell command requires approval in workspace-edit mode:

cargo test

Press y to approve, n to deny, Esc to deny, Ctrl-C to cancel the turn.
```

Do not execute the shell command until the approve decision reaches the agent.

## CLI and config changes

### CLI

Add:

```text
--workspace-edit
```

It should conflict with `--readonly` and `--full-access`.

### Config

Allow:

```json
{
  "default_access_mode": "workspace-edit"
}
```

Config loading should still default to `read-only` unless project direction changes.

### Mode cycling

`Shift-Tab` should cycle:

```text
read-only -> workspace-edit -> full-access -> read-only
```

Only while idle, preserving current behavior.

## Prompt updates

Update `src/prompt.rs` to describe each mode accurately.

Key requirements:

- Include `workspace-edit` in access-mode instructions.
- Tell the model that shell commands in `workspace-edit` require user confirmation and should be requested only when useful.
- Do not promise that prompt rules are the only enforcement.
- Update allowed tool list generation to use policy availability.

Example text for the new mode:

```text
In workspace-edit mode, you may inspect files with ls, read, and grep only inside the launch working directory or bundled Cass docs directory. You may write and edit files only inside the launch working directory. Bundled Cass docs are read-only. You may request shell when needed, but Cass will ask the user for confirmation before executing the command.
```

Update tool descriptions:

- `write`: no longer says only `Requires full-access mode`; say it requires an access mode that permits writes and is subject to policy path restrictions.
- `edit`: same.
- `shell`: say it may require user confirmation depending on active access mode.

## Conversation and provider correctness

If an approval is denied, append a `Record::Tool` with:

- matching `tool_call_id`
- `name` equal to the requested tool name
- `ok: false`
- content explaining that the user denied or policy denied the tool

This is important so the next provider request has valid assistant-tool message structure. Do not drop denied tool calls.

Approval events are UI/runtime-only and should not be persisted as separate conversation records in v1. The failed or successful tool result is sufficient persistent state.

## Testing plan

### Unit tests

Add or update tests for `AccessMode`:

- serde supports `workspace-edit`
- `as_str()` returns `workspace-edit`
- `next()` cycles through all three modes

Add security policy tests:

- read-only allows read under workspace
- read-only allows read under docs
- read-only denies write/edit/shell
- read-only denies read outside workspace/docs
- workspace-edit allows write under workspace
- workspace-edit denies write outside workspace
- workspace-edit denies write under docs
- workspace-edit asks for shell
- full-access allows shell
- full-access still denies writes under docs
- symlink escape writes are denied in workspace-edit
- new file under symlinked parent outside workspace is denied

### Tool tests

Update `tests/tool_tests.rs`:

- `workspace-edit` exposes `write`, `edit`, and `shell`
- write inside workspace succeeds in `workspace-edit`
- edit inside workspace succeeds in `workspace-edit`
- write outside workspace fails in `workspace-edit`
- edit outside workspace fails in `workspace-edit`
- shell without prior approval does not execute in `workspace-edit` if using defensive `tools::execute`
- read-only behavior is unchanged

### Agent tests

Add tests with a mock provider/tool-call sequence:

- shell approval request is emitted in `workspace-edit`
- no shell output occurs before approval
- approved shell executes and appends successful tool result
- denied shell appends failed tool result and continues model loop
- policy-denied write appends failed tool result without executing
- cancellation while approval is pending leaves conversation resumable

### UI/manual tests

Manual checklist:

- Start Cass in default read-only mode.
- Press `Shift-Tab` once: status/header shows `workspace-edit`.
- Press `Shift-Tab` again: status/header shows `full-access`.
- In workspace-edit, ask Cass to edit a file under cwd; no confirmation appears and file changes.
- In workspace-edit, ask Cass to run `pwd`; approval prompt appears before execution.
- Press `n`; command does not run and model receives denial result.
- Ask again and press `y`; command runs and output streams.
- Try to edit bundled Cass docs; denied.
- Try a symlink escape write; denied.

## Migration strategy

Implement in small, testable phases.

### Phase 1: Mode and config plumbing

- Add `AccessMode::WorkspaceEdit`.
- Add `--workspace-edit` CLI flag.
- Update config parsing and defaults.
- Update UI mode cycle and status/header rendering.
- Add serde/cycle tests.

This phase may temporarily map `workspace-edit` to existing full-access behavior only in code behind tests, but should not ship until policy enforcement is complete.

### Phase 2: Central policy layer

- Add `src/security.rs`.
- Build `SecurityContext` from app/agent config.
- Implement tool availability and path decisions.
- Add comprehensive policy tests.

### Phase 3: Refactor tool gating

- Update `tools::available_tool_names` and `tools::specs` to use `SecurityContext`.
- Add policy checks to `tools::execute` as a defensive enforcement point.
- Remove direct `ctx.mode.can_write()` checks from tools.
- Update path resolution to use policy helpers.
- Update existing tool tests.

### Phase 4: Approval channel and UI

- Add `AgentCommand` channel.
- Add `ApprovalRequested` event.
- Gate `Ask` decisions before execution in `agent.rs`.
- Add pending approval UI handling in `app.rs`.
- Add agent approval tests.

### Phase 5: Prompt, docs, and polish

- Update `src/prompt.rs`.
- Update tool descriptions.
- Update README/config docs as needed.
- Update `ROADMAP.md` status and any manual testing instructions.
- Run `cargo fmt`, `cargo test`, and a manual TUI smoke test.

## Future extensions enabled by this refactor

Once the policy layer exists, additional restrictions can be added without rewriting every tool:

- Configurable confirmation for destructive full-access operations.
- Deny or ask before touching `.env`, SSH keys, credentials, or lockfiles.
- Protect `.git` internals while allowing normal git commands through approved shell.
- Add per-project policy config.
- Add temporary approvals for a single turn or command pattern.
- Add audit logging for approved/denied actions.
- Add sandboxed shell execution in a later release.
- Add fine-grained modes such as `workspace-read-global-shell-ask` without changing tool internals.

## Open decisions

- Should `workspace-edit` become the default mode after it is stable, or should default remain `read-only`?
- Should `full-access` shell remain auto-allowed, or should shell confirmation be configurable globally?
- Should approvals support `approve once`, `approve all shell for this turn`, or only one-command approval in the first implementation?
- Should `workspace-edit` allow reading outside workspace with confirmation, or always deny?
- Should policy decisions be persisted in an audit log separate from conversation JSONL?
