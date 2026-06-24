# Platform notes

Cassady is a terminal CLI. Most behavior is shared across platforms, but environment-variable syntax, paths, and terminal behavior differ.

## macOS and Linux

Set an API key for the current shell:

```sh
export OPENAI_API_KEY=...
cass check
cass
```

Use normal POSIX paths:

```sh
cass --cwd /Users/alex/project
cass --cwd /home/alex/project
```

Shell tools run through `sh -c` from the launch cwd.

## Windows

Release builds include a Windows x86_64 binary. Use PowerShell syntax for environment variables:

```powershell
$env:OPENAI_API_KEY = "..."
cass check
cass
```

Example path usage:

```powershell
cass --cwd C:\Users\alex\project
```

Current docs and examples are primarily terminal-CLI oriented. Deeper Windows polish for terminal behavior, path handling, shell behavior, filesystem edge cases, and release usability is planned for a later release. Avoid assuming every Windows path or shell edge case is polished in the current version.

## Config location

Cassady currently stores config under the home directory at:

```text
~/.cass
```

That directory contains `config.json`, `providers.json`, `models.json`, `global.md`, `conversations/`, and installed bundled docs.

## Non-interactive contexts

- `cass check` is suitable for scripts and CI because it prints text and exits non-zero on errors.
- `cass setup` requires an interactive terminal.
- `cass` chat is an interactive terminal UI.

## Release artifacts

When using release archives, each archive contains both `cass` and `cassady`. Put the extracted binaries somewhere on your `PATH` or run them by explicit path. Cassady itself does not install, update, or manage PATH entries.
