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

Cassady can use `ls`, `read`, and `grep` to inspect the workspace and bundled docs.

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

## Switch model

Inside a chat:

```text
/model MODEL_ID
```

Autocomplete lists models from `~/.cass/models.json`. Switching models is allowed only when idle. Cassady persists the last used model and reasoning effort into `config.json`.

You can also launch with a model override:

```sh
cass --model MODEL_ID
```

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

Prefer `cass setup` for provider/model changes when possible.
