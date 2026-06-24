# Access modes and tool safety

Cassady exposes tools according to the active access mode. Choose a mode at startup with `--readonly`, `--workspace-edit`, or `--full-access`, or press `Shift-Tab` while idle to cycle modes.

The launch cwd is the current directory unless `--cwd PATH` is provided. In read-only and workspace-edit modes, that cwd is the workspace root.

## Tool matrix

| Tool area | read-only | workspace-edit | full-access |
| --- | --- | --- | --- |
| List/read/grep workspace files | yes | yes | yes |
| Read bundled docs under `~/.cass/docs` | yes | yes | yes |
| Write/edit workspace files | no | yes | yes |
| Write/edit bundled docs | no | no | no |
| Shell commands | no | approval required | yes |
| Read outside workspace/docs | no | no | yes |
| Write outside workspace | no | no | yes, except bundled docs |

## Tools

- `ls`: list files.
- `read`: read file contents.
- `grep`: search file contents.
- `write`: create or overwrite files when writes are allowed.
- `edit`: apply exact old-text/new-text replacements when writes are allowed.
- `shell`: run `sh -c` in the launch cwd with an optional timeout, defaulting to 30 seconds.

Shell output is streamed into the transcript while the command runs. The final shell result includes stdout, stderr, and exit code. Timed-out commands are killed and reported as failures.

## Read policy

In `read-only` and `workspace-edit`, Cassady can read only:

- the launch workspace root; and
- the installed bundled docs directory.

A path that resolves outside those roots is denied with a message like:

```text
path escapes read-only roots: /path/outside (allowed roots: ...)
```

In `full-access`, read/list/search actions are allowed subject to normal OS permissions.

## Write policy

In `read-only`, write and edit tools are unavailable.

In `workspace-edit`, write and edit tools are allowed only inside the launch workspace. Paths that resolve outside the workspace are denied with a message like:

```text
write path escapes workspace-edit root: /path/outside (workspace root: ...)
```

In `full-access`, write and edit tools are allowed broadly subject to OS permissions, but writes under the bundled docs directory are still blocked:

```text
writes are blocked under read-only docs directory: ...
```

`write` uses atomic writes where practical. `edit` requires every `old_text` to match exactly once in the original file and rejects overlapping replacements.

## Shell approvals and destructive-operation setting

- `read-only`: shell is unavailable.
- `workspace-edit`: shell requires a UI approval prompt. Press `y` to approve, `n` or `Esc` to deny.
- `full-access`: shell is allowed by policy without the workspace-edit approval prompt.

`config.json` accepts `confirm_destructive_operations` as a stored compatibility preference, but the current runtime policy is the access-mode and shell-approval behavior described above.

If approval is denied, the tool result says:

```text
user denied approval for this tool call
```

## Practical guidance

- Start in `read-only` when asking for explanations or audits.
- Use `workspace-edit` for normal coding work in a repository.
- Use `full-access` only when you intentionally want Cassady to operate outside the launch workspace or run shell commands without the approval prompt.
- Review tool call output and diffs before continuing after edits.
- Keep secrets in environment variables; do not ask Cassady to write literal API keys into project files.
