# Troubleshooting

Use `cass check` first for configuration problems. It validates files, provider/model references, and API key availability.

## Missing active API key

Symptom: `cass check` reports that an environment variable is not set, or chat startup says the API key is not available.

Likely cause: the active provider's `api_key` is an env reference such as `"$OPENAI_API_KEY"`, but that variable is not set in the current shell.

Fix on macOS/Linux:

```sh
export OPENAI_API_KEY=...
cass check
cass
```

Fix in PowerShell:

```powershell
$env:OPENAI_API_KEY = "..."
cass check
cass
```

## Invalid API key reference

Symptom: the key is not resolved the way you expect.

Likely cause: Cassady only treats strings that start with `$` as environment references. It does not expand partial strings or `${NAME}` syntax.

Fix:

```json
{ "api_key": "$OPENAI_API_KEY" }
```

## Provider URL unreachable

Symptom: setup model discovery fails or a turn reports a provider error.

Likely causes:

- wrong `base_url`;
- network or proxy problem;
- provider outage;
- provider requires a different OpenAI-compatible path.

Fix: verify the base URL in `providers.json`, retry setup, or enter the model id manually if only `/models` discovery is failing.

## `/models` discovery fails

Symptom: setup cannot fetch models.

Likely cause: some providers do not expose `GET /models`, require extra permissions, or return a non-standard shape.

Fix: choose retry if the failure is temporary; otherwise enter the model id manually. Then run `cass check`.

## Unsupported or invalid model id

Symptom: chat starts but the provider rejects the model.

Likely cause: the model id in `models.json` or `config.json` is not valid for the provider.

Fix: update the model id with `cass setup`, edit `models.json`, or launch with:

```sh
cass --model MODEL_ID
```

Then verify with a small prompt.

## Rate limit or authentication errors

Symptom: the assistant says the provider returned an error.

Likely cause: provider-side authentication, quota, billing, or rate limit.

Fix: confirm the API key, provider account status, selected model, and provider dashboard. Cassady forwards provider failures into the chat but cannot resolve account-level issues.

## Invalid JSON or unknown config fields

Symptom: `cass check` reports a parsing or schema error for `config.json`, `providers.json`, or `models.json`.

Likely cause: invalid JSON, comments, trailing commas, misspelled fields, or a field in the wrong file.

Fix: remove comments/trailing commas, compare against [Configuration](configuration.md), and run:

```sh
cass check
```

## Workspace access denied

Symptom: tool output says a path escapes allowed roots or workspace-edit root.

Likely cause: the active mode is `read-only` or `workspace-edit`, and the path resolves outside the launch cwd or bundled docs.

Fix: start from the intended project directory, pass `--cwd PATH`, or intentionally use `--full-access` when broad filesystem access is needed.

## Shell unavailable or waiting for approval

Symptom: shell is denied or Cassady asks for approval.

Rules:

- `read-only`: shell is unavailable.
- `workspace-edit`: shell requires approval.
- `full-access`: shell is allowed by policy.

Fix: switch mode with `Shift-Tab` while idle or launch with the desired access flag.

## Shell command failed or timed out

Symptom: shell result includes stderr, non-zero exit code, or timeout.

Likely cause: the command itself failed, the working directory is wrong, dependencies are missing, or the timeout was too short.

Fix: inspect stdout/stderr, verify cwd in `/status`, and ask Cassady to rerun the smallest relevant command.

## Exact-text edit failed

Symptom: edit reports `old_text not found`, `old_text is not unique`, or overlapping edits.

Likely cause: the file changed, whitespace differs, line endings differ, or the replacement text is too broad.

Fix: ask Cassady to re-read the file and retry with a smaller unique `old_text`. For repeated blocks, include nearby unique context.

## Binary, large, or unsupported files

Symptom: read/edit output is confusing or fails.

Likely cause: the file is binary, too large for useful display, or not valid UTF-8 for text edits.

Fix: ask Cassady to list metadata or use project-specific tools through approved shell commands. Avoid direct text edits on binary files.

## CRLF or line-ending confusion

Symptom: exact-text edits fail even when the text appears to match.

Likely cause: Windows CRLF line endings or invisible whitespace differences.

Fix: re-read the exact target region and preserve the line endings in `old_text`, or use a smaller unique snippet.

## Branch/restore file conflicts

Symptom: branch-plus-file restore reports conflicts or skips paths.

Likely cause: the file changed outside Cassady after the tracked `write`/`edit`, the file is unsupported for snapshots, or the change came from a shell command or manual editor rather than a Cassady file tool.

Fix: review the restore preview, inspect conflicted files manually, and rerun the menu with conversation-only branching if you only need to revisit the chat. Cassady will not overwrite unknown current content by default. Open the branch/restore menu again with double `Esc` or `/branch` to switch back to the original branch.

## Update command problems

Symptom: `cass update` cannot complete.

Likely causes and fixes:

- Network or GitHub API failure: retry later or verify proxy/firewall settings.
- No matching prebuilt archive: use `cass update --source` if you have Rust installed, or download the release archive manually for a supported target.
- SHA-256 mismatch: do not install the archive. Retry the update; if it repeats, check the GitHub release page before proceeding.
- Missing Rust toolchain in source mode: install Rust/Cargo yourself, then rerun `cass update --source`. Cassady does not install Rust automatically.
- Non-writable install directory: update through the original install method, move Cassady to a directory you own, or manually replace the binaries. Cassady does not run `sudo` for you.
- PATH conflict: `cass update` updates the current executable directory. Run `which cass` / `which cassady` on macOS/Linux or `Get-Command cass` in PowerShell to confirm which binary your shell starts.
- Windows replacement limitation: if Cassady reports that automatic replacement is unavailable, use the staged file paths it prints and copy them after the running process exits.

Useful checks:

```sh
cass update --check
cass update --dry-run
cass --version
cassady --version
```

## Terminal rendering problems

Symptom: the UI appears garbled or keys do not behave as expected.

Likely cause: unsupported terminal features, redirected stdin/stdout, or platform-specific terminal behavior.

Fix: run Cassady in an interactive terminal. Use `cass check` for non-interactive validation. Windows runtime polish is planned for a later release.

## Setup says it is interactive

Symptom: `cass setup` fails with `setup is interactive; run cass setup in a terminal`.

Likely cause: stdin is not a terminal.

Fix: run setup directly in a terminal, not through a non-interactive script or redirected input.
