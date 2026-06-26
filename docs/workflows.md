# Workflows

This page shows common Cassady workflows. Exact tool calls depend on the model and the prompt; use these examples as patterns rather than scripts.

## Start in a workspace

```sh
cd /path/to/project
cass
```

Or choose a workspace explicitly:

```sh
cass --cwd /path/to/project
```

Ask for read-only exploration first:

```text
Explain the structure of this repository. Do not edit files yet.
```

## Inspect and explain code

Start in read-only mode or press `Shift-Tab` until the status shows `read-only`.

```text
Find where configuration is loaded and summarize the precedence rules.
```

Cassady can use `ls`, `read`, and `grep` to inspect the workspace and bundled docs. For large files or unknown locations, prefer a search-first flow: `grep` for a symbol or phrase, then `read` a small line range around the relevant match.

## Recover from compacted or truncated output

When a tool result is too large for the model context, Cassady sends the model a head/tail excerpt with a recovery note. The note includes the tool name, retained excerpt shape, and file range or shell-command provenance when available.

If Cassady reports compacted or truncated output, do not rely on omitted lines for edits. Ask Cass to narrow the inspection instead:

```text
Re-read src/app.rs lines 220-280 before editing that function.
```

```text
Search only src/ for "load_config" and then read around the matching lines.
```

```text
Rerun the test command with a focused package/filter, or pipe the noisy output through grep/head/tail.
```

## Apply a focused edit

Use workspace-edit mode:

```sh
cass --workspace-edit
```

Then ask for a precise change:

```text
Update the README install section to mention both cass and cassady. Keep the rest unchanged.
```

Cassady may use `edit` or `write`. Edit results include a diff-like summary. If an exact-text edit fails, ask Cassady to re-read the file and retry with a smaller unique replacement.

## Run tests or builds

Shell is denied in read-only mode and requires approval in workspace-edit mode.

```text
Run the smallest relevant Rust test for this change, then summarize the result.
```

When the approval prompt appears, press `y` to approve or `n`/`Esc` to deny. Shell commands run with `sh -c` from the launch cwd and default to a 30-second timeout unless the model requests another timeout.

## Manage provider login

Add or update provider configuration from the shell:

```sh
cass login
```

Inside an idle chat:

```text
/login
```

Remove saved provider configuration:

```sh
cass logout
```

Inside an idle chat:

```text
/logout
```

Logout removes selected providers from Cassady's config and removes their associated model entries. It does not delete environment variables, local Codex auth, or external provider accounts.

For `ChatGPT Codex`, run `codex login` or sign in with the Codex app before `cass login`. Cassady validates `~/.codex/auth.json` and uses that local token source instead of asking for an API-key environment variable.

## Switch model

Inside a chat:

```text
/model MODEL_ID
```

Autocomplete lists models from `~/.cass/models.json`. Switching models is allowed only when idle. Cassady persists the last used provider, model, and reasoning effort into `config.json`.

You can also launch with a model override:

```sh
cass --model MODEL_ID
```

## Prefer fast mode

Inside a chat:

```text
/fast
```

Use `/fast on`, `/fast off`, or `/fast status` when you want an explicit action. Cassady saves the preference in `config.json`, but fast mode is active only when the current provider/model supports it. ChatGPT Codex models, including `gpt-5.5`, are treated as fast-capable.

Switching to an unsupported provider/model keeps the preference but makes `/status` show fast mode as unavailable. Switching back to ChatGPT Codex enables it again.

## Resume a chat

List chats for the current directory:

```sh
cass --resume
```

Resume a specific chat:

```sh
cass --resume CHAT_ID
```

Inside the UI:

```text
/resume CHAT_ID
```

`/resume` autocomplete lists saved chats for the current directory.

## Start fresh without leaving

```text
/new
```

This creates a new chat for the same cwd and model while preserving your current configuration.

## Branch or restore a conversation point

Press `Esc` twice while idle, or type:

```text
/branch
```

Use the menu to select a related branch or a checkpoint from a user message, assistant message, tool-call request, or tool result. Branching creates a new chat from that point and leaves the original chat available in the same menu. Choose conversation-only branching to leave files untouched, or choose branch-plus-files to restore Cassady-tracked `write`/`edit` snapshots with conflict checks.

## Check status

```text
/status
```

The status block includes chat id, state, model, mode, cwd, record count, and the current status message.

## Cancel and continue

While a turn is running:

- Press `Esc` or `Ctrl-C` to request turn cancellation.
- Press `Ctrl-C` twice within 1.5 seconds to exit.

Cassady records cancelled tool calls and a cancellation message so the conversation can continue cleanly.

## Ask Cassady to edit its config

Config files live under `~/.cass`, outside a normal project workspace. To inspect them, use `full-access` or edit them manually. After manual changes, run:

```sh
cass check
```

For OpenAI-compatible providers this checks API key environment variables. For `ChatGPT Codex` this checks local Codex auth and points you back to `codex login` if the token is missing or expired. Prefer `cass setup` or `cass login` for provider/model changes when possible.
