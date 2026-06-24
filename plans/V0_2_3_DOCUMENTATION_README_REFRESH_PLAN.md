# v0.2.3 Documentation and README Refresh Implementation Plan

## Goal

v0.2.3 refreshes Cassady's user-facing documentation so a new or returning user can understand what Cassady does, configure it successfully, use it safely in a project, and recover from common failures without reading the source. The README should become the polished entry point, while bundled docs under `docs/` should provide accurate reference material that the CLI can install into `~/.cass/docs`.

Success statement:

> A user can start from the README, run the documented setup/check/chat commands, understand the current provider/model/access-mode model, and find accurate troubleshooting guidance for the shipped v0.2.3 CLI.

## Scope

### In scope

- Rewrite `README.md` around the current Cassady experience.
- Refresh bundled docs in `docs/`, which are embedded by `src/docs.rs` and installed to `~/.cass/docs`.
- Add or split reference docs for commands, configuration, providers/models, access modes/tool safety, workflows, troubleshooting, platform notes, and glossary terms.
- Audit and update CLI help text in `src/cli.rs` only where it contradicts or underspecifies documented behavior.
- Verify documented commands and examples against the actual CLI behavior.
- Add lightweight documentation tests where practical, especially for bundled-doc presence and link integrity.
- Keep documentation for both command names: `cass` and `cassady`.
- Clearly describe current limitations and defer deep Windows runtime improvements to v0.2.4.

### Out of scope

- Broad CLI feature work or behavior changes beyond correcting inaccurate help text.
- Windows terminal, filesystem, shell, and process usability fixes planned for v0.2.4.
- Installer, package manager, code signing, auto-update, or PATH setup documentation that implies unsupported release channels.
- Adding new provider protocols such as Anthropic-native APIs.
- Reworking config formats or access-mode policy implementation.
- Creating exhaustive model catalogs for providers.

## Context and Current State

Relevant files:

- `README.md`: current root overview; accurate in places but short and reference-heavy.
- `docs/README.md`: bundled-doc index installed at runtime.
- `docs/configuration.md`: current bundled configuration/setup reference; includes substantial v0.2.2 setup details.
- `src/docs.rs`: embeds all files in `docs/`; changing docs changes the build-time docs hash.
- `src/cli.rs`: Clap definitions for global flags and `check`/`setup` subcommands.
- `src/check.rs`: rendered `cass check` output and next-step wording.
- `src/setup.rs`: setup wizard prompts and provider catalog.
- `src/config.rs`: config file schema, defaults, precedence, provider/model resolution.
- `src/access.rs`, `src/security.rs`, and `src/tools/*`: access-mode/tool behavior that docs must describe accurately.
- `src/app.rs`, `src/ui/render.rs`, `src/ui/events.rs`: chat commands, keys, rendering, cancellation, tool display, reasoning toggles.
- `tests/docs_tests.rs`: current docs-related test coverage.

Existing documentation facts to preserve when accurate:

- Cassady ships two binaries, `cass` and `cassady`.
- `cass` starts chat by default; `cass setup` runs the setup wizard; `cass check` validates config non-interactively.
- Config lives under `~/.cass` today, with bundled docs installed to `~/.cass/docs`.
- Provider/model configuration uses `config.json`, `providers.json`, and `models.json`.
- OpenAI-compatible providers are the only supported provider kind.
- API keys should usually be environment-variable references such as `"$FIREWORKS_API_KEY"`.
- Access modes are `read-only`, `workspace-edit`, and `full-access`.
- Tool output is compact by default; `Ctrl-O` toggles full tool output.
- Reasoning is hidden by default; `Ctrl-Shift-R` toggles display and `Tab` cycles effort.

## Design Principles

1. **Docs are product UX.** Write polished explanatory prose, not a dump of implementation checklists.
2. **Verify before claiming.** Every command, flag, env var, provider URL, config field, keybinding, and output snippet should be checked against the code or an actual run.
3. **README first, references second.** Keep `README.md` focused on orientation, first use, common workflows, and links; move long tables and detailed references into `docs/`.
4. **One terminology set.** Use consistent names: Cassady/Cass, workspace, session/chat, provider, model, access mode, tool call, tool result, setup wizard, bundled docs.
5. **Avoid future promises.** Mention Windows limitations and planned v0.2.4 work without promising unimplemented terminal/process/path behavior.
6. **Security-forward but practical.** Explain what Cassady can read, write, and run before encouraging users to grant broader access.

## Documentation Architecture

Use this structure unless implementation reveals a simpler split is better:

```text
README.md

docs/README.md
docs/commands.md
docs/configuration.md
docs/providers.md
docs/access-modes.md
docs/workflows.md
docs/troubleshooting.md
docs/platforms.md
docs/glossary.md
```

### Root README role

`README.md` should be the public landing page and fast-start guide. Suggested sections:

1. `# Cassady / Cass`
2. Short product summary and current limitations.
3. Install from source for development/current release usage.
4. First use walkthrough.
5. Everyday workflows.
6. Safety model summary.
7. Commands and key shortcuts summary.
8. Configuration and providers summary with links.
9. Troubleshooting quick links.
10. Bundled docs and repository docs map.

Keep the README concise enough to read top-to-bottom. Move long provider tables, config schemas, and troubleshooting matrices into bundled docs.

### Bundled docs role

Bundled docs are installed to `~/.cass/docs` and are accessible to Cassady's docs tools. They should be self-contained enough to help a user inside a session.

- `docs/README.md`: index with short descriptions and links to all bundled docs.
- `docs/commands.md`: complete CLI command, flag, alias, startup, interactive-vs-non-interactive, resume, and output-mode reference.
- `docs/configuration.md`: config file locations, schemas, examples, precedence, validation, safe editing.
- `docs/providers.md`: built-in OpenAI-compatible provider catalog, custom providers, model discovery, manual model entry, health checks, unsupported protocols.
- `docs/access-modes.md`: read/write/edit/shell/docs access by mode, approvals, denied examples, workspace boundaries, symlink notes, diff review.
- `docs/workflows.md`: task-oriented examples for chat, file inspection, edits, test/build commands, model switching, cancellation recovery.
- `docs/troubleshooting.md`: actionable fixes for setup, provider/API, config, terminal, shell, edit, access, and line-ending failures.
- `docs/platforms.md`: macOS/Linux/Windows notes, env var examples, path examples, known Windows limitations, no installer promises.
- `docs/glossary.md`: definitions for recurring concepts.

## Detailed Content Requirements

### README rewrite

Include a current, concise product description:

```md
Cassady (`cass`) is a terminal coding agent written in Rust. It runs an interactive chat in your project, can inspect files, propose and apply edits, run approved shell commands, and persist sessions for later resume. It currently talks to OpenAI-compatible providers.
```

Document limitations explicitly:

- Only OpenAI-compatible chat/completions-style providers are supported.
- Built-in docs and examples assume a terminal CLI workflow.
- Windows support exists through cross-built binaries but deep Windows runtime polish is planned for v0.2.4.
- Cassady is not an installer/updater/package manager.

### First-use walkthrough

Show a linear path:

```sh
cass
# or explicitly:
cass setup
cass check
cass
```

Cover:

- First-run setup trigger when active provider/model/API key cannot be resolved.
- Provider selection from built-ins or custom OpenAI-compatible endpoint.
- API key env vars, with POSIX and PowerShell examples labelled clearly.
- Model discovery via `GET /models`, retry, and manual model id fallback.
- What happens if setup writes config but the API key is still missing.
- How to recover with `cass setup` and `cass check`.

### Command reference

Document these top-level forms based on `src/cli.rs`:

```sh
cass [OPTIONS]
cassady [OPTIONS]
cass check [OPTIONS]
cass setup [OPTIONS]
cass --resume [CHAT_ID]
```

Document global options:

- `--resume [CHAT_ID]`
- `--model MODEL`
- `--base-url URL`
- `--api-key-env ENV`
- `--cwd PATH`
- `--readonly`
- `--workspace-edit`
- `--full-access`
- `--help`
- `--version`

Also document in-chat commands and keys from current behavior, including `/model`, `/new`, `/resume`, `/status`, `/`, `Tab`, `Shift-Tab`, `Ctrl-O`, `Ctrl-Shift-R`, scrolling, multiline input, and double `Ctrl-C` exit. Verify exact command names in code before finalizing.

### Configuration reference

Keep `docs/configuration.md` as the canonical config reference. It should explain:

- Location under `~/.cass` and current portability caveat.
- `config.json`, `providers.json`, `models.json` responsibilities.
- Default provider/model behavior and compatibility fields.
- Precedence between config files, CLI overrides, setup wizard changes, and env vars.
- API key reference syntax: only strings beginning with `$` are env refs; no partial expansion or `${NAME}` syntax unless code supports it.
- Safe manual edits and `cass check` validation.
- Valid and invalid JSON examples with fixes.

### Provider and model guide

Create `docs/providers.md` and move long provider details there. Include the v0.2.2 provider catalog:

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

Also explain:

- Provider configuration vs model metadata vs active defaults.
- Model discovery limits and manual model id entry.
- `supports_tools`, `supports_streaming`, and reasoning metadata in user-facing terms.
- Unsupported provider protocols and what a custom OpenAI-compatible provider must implement.

### Access modes and tool safety

Create `docs/access-modes.md`. Include a mode/tool matrix such as:

| Tool area | read-only | workspace-edit | full-access |
| --- | --- | --- | --- |
| List/read/grep workspace files | yes | yes | yes |
| Write/edit workspace files | no | yes | yes |
| Read bundled docs | yes | yes | yes |
| Write bundled docs | no | no | no |
| Shell commands | no or denied unless code says otherwise | approval required | approval/destructive confirmation as implemented |
| Outside workspace | no | no | allowed subject to OS permissions |

Verify exact shell availability and approval behavior from `src/access.rs`, `src/security.rs`, and `src/tools/shell.rs` before publishing this table.

Include denied-operation examples with realistic wording based on actual errors, not invented output if the code differs.

### Workflows and examples

Create `docs/workflows.md` with short examples for:

- Starting a chat in a workspace.
- Asking Cassady to inspect files and explain code.
- Asking for a proposed edit, reviewing tool calls, and applying edits.
- Running tests or builds with shell approval.
- Switching model with `/model <model>`.
- Resuming sessions with `cass --resume` and `/resume`.
- Cancelling a turn and continuing cleanly.

Examples should be realistic but not overly long. Avoid implying that Cassady will always make a specific sequence of tool calls.

### Troubleshooting

Create `docs/troubleshooting.md` organized by symptom. Include:

- Missing active API key.
- Invalid env var references.
- Provider URL unreachable.
- `/models` discovery failure.
- Unsupported/invalid model id.
- Rate limit/authentication errors.
- Invalid JSON or unreadable config files.
- Terminal rendering issues and redirected output caveats.
- Shell command failures and approval/cancellation behavior.
- Exact-text edit failures.
- Binary/large/unsupported files.
- CRLF/line-ending confusion.
- Workspace access denials and symlink/bundled-doc restrictions.

Each entry should include: symptom, likely cause, fix, and command to verify when applicable.

### Platform notes

Create `docs/platforms.md` with careful current-state language:

- macOS/Linux examples can use POSIX shell syntax such as `export NAME=...`.
- Windows examples should be labelled and use PowerShell syntax such as `$env:OPENAI_API_KEY = "..."`.
- Document Windows path examples without claiming every Windows path edge case is polished.
- State that v0.2.4 is planned to improve Windows terminal, path, shell, and filesystem behavior.
- Avoid installation-channel promises beyond current source/release-artifact facts.

## Implementation Steps

1. **Inventory current behavior.**
   - Run `cargo run -- --help`, `cargo run -- check --help`, and `cargo run -- setup --help`.
   - Review `src/cli.rs`, `src/setup.rs`, `src/config.rs`, `src/access.rs`, `src/security.rs`, `src/tools/*`, and relevant UI command/key handling.
   - Record exact command names, flags, config fields, provider catalog entries, access rules, and keybindings.

2. **Design the docs map.**
   - Confirm the final bundled docs file list.
   - Decide which details stay in `README.md` and which move to `docs/`.
   - Keep `docs/README.md` as the navigable index.

3. **Rewrite `README.md`.**
   - Replace stale MVP/default-only language with current v0.2.2+ behavior.
   - Add first-use walkthrough, everyday workflows, safety summary, limitations, and links to detailed docs.
   - Keep examples copy/paste-ready and label platform-specific syntax.

4. **Refresh bundled reference docs.**
   - Update `docs/configuration.md` instead of duplicating schema details elsewhere.
   - Add `docs/commands.md`, `docs/providers.md`, `docs/access-modes.md`, `docs/workflows.md`, `docs/troubleshooting.md`, `docs/platforms.md`, and `docs/glossary.md` as needed.
   - Update `docs/README.md` links and summaries.

5. **Synchronize CLI help text.**
   - Make minimal edits to `src/cli.rs` descriptions if help text conflicts with the refreshed docs.
   - Do not change command behavior in this release unless a documentation verification step uncovers a severe typo or misleading help string.

6. **Add lightweight docs validation.**
   - Extend `tests/docs_tests.rs` or add a new docs test to ensure all linked bundled docs exist.
   - Consider checking that `docs/README.md` links are relative and valid.
   - If practical, add a test that important terms or command names appear in the bundled docs index.

7. **Verify examples.**
   - Run documented help/check/setup commands where safe.
   - Use a temporary Cass root or environment isolation for config examples when possible.
   - Verify internal links and fenced command snippets manually or with tests.

8. **Final consistency pass.**
   - Search for old terminology, obsolete commands, stale provider data, and outdated MVP wording.
   - Ensure README, bundled docs, CLI help, and roadmap use the same terms.
   - Run formatting/tests.

## Tests and Verification

Automated checks:

```sh
cargo fmt --check
cargo test --locked --all-targets
```

Docs-specific checks to add or perform:

- `tests/docs_tests.rs` validates bundled docs install/embedding behavior still passes.
- New or updated test validates `docs/README.md` links resolve to existing bundled docs files.
- `cargo run -- --help` output matches `docs/commands.md`.
- `cargo run -- check --help` and `cargo run -- setup --help` output are documented accurately.
- `cargo run -- check` behavior is represented accurately, preferably with a temp config root if the code supports test helpers.

Manual review checklist:

- Every internal Markdown link works.
- Every command name exists.
- Every flag is spelled exactly as Clap exposes it.
- Every provider URL/env var matches setup's provider catalog.
- Every config field in examples is accepted by the current parser.
- Access-mode descriptions match current policy code.
- Windows notes are cautious and do not include v0.2.4 promises as current behavior.

## Documentation Deliverables

Required:

- Updated `README.md`.
- Updated `docs/README.md`.
- Updated `docs/configuration.md`.
- New command/provider/access/workflow/troubleshooting/platform/glossary docs, unless the implementer chooses a smaller file split and preserves all required content.
- Any necessary tiny CLI help text corrections.
- Updated docs tests.

Not required:

- Release notes, unless the release process is being run.
- Website docs.
- Generated `dist/` artifacts.

## Acceptance Criteria

- `README.md` accurately describes current Cassady behavior and guides first use from setup through first chat.
- Bundled docs provide complete references for commands, config, providers/models, access modes/tool safety, workflows, troubleshooting, platforms, and glossary concepts.
- Documentation covers both `cass` and `cassady` command names.
- Provider catalog, API key env vars, config examples, access mode descriptions, and keybindings match the code.
- Windows documentation is accurate but clearly defers deep Windows runtime polish to v0.2.4.
- Obsolete MVP language, stale commands, and misleading defaults are removed.
- Internal links resolve and docs tests cover bundled-doc navigation where practical.
- `cargo fmt --check` and `cargo test --locked --all-targets` pass before release handoff.
