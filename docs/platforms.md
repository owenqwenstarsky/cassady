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

Current docs and examples are primarily terminal-CLI oriented. The desktop preview is not packaged as a native Windows desktop executable; use the Linux binary under WSL/WSLg if you want to try it on Windows for now. Deeper Windows polish for terminal behavior, path handling, shell behavior, filesystem edge cases, and release usability is planned for a later release. Avoid assuming every Windows path or shell edge case is polished in the current version.

## Config location

Cassady currently stores config under the home directory at:

```text
~/.cass
```

That directory contains `config.json`, `providers.json`, `models.json`, `global.md`, `conversations/`, and installed bundled docs.

## Non-interactive contexts

- `cass check` is suitable for scripts and CI because it prints text and exits non-zero on errors.
- `cass update --check` and `cass update --dry-run` are suitable for scripts that only need release status or an install plan.
- `cass update --yes` accepts default prompts for scripted updates, but still fails instead of escalating privileges when the install directory is not writable.
- `cass setup` requires an interactive terminal.
- `cass` chat is an interactive terminal UI.

## Release artifacts and updates

When using release archives, each archive contains both `cass` and `cassady`. macOS and Linux archives also contain a release-built `cassady-desktop` with bundled frontend assets; put all extracted binaries somewhere on your `PATH` or keep them in the same directory so `cass desktop` can launch the desktop app.

`cass update` can update release-archive installs from official GitHub releases. It supports the same prebuilt targets as the release process:

- macOS Apple Silicon: `aarch64-apple-darwin`
- Linux x86_64: `x86_64-unknown-linux-gnu`
- Linux ARM64: `aarch64-unknown-linux-gnu`
- Windows x86_64: `x86_64-pc-windows-gnu`

On macOS and Linux, the updater replaces same-directory `cass` and `cassady` binaries with backups and rollback on failure. For prebuilt archives that include `cassady-desktop`, it installs that companion binary too. On Windows, replacing a running `.exe` is more constrained; if automatic replacement is unavailable, Cassady leaves staged files in place and reports manual copy guidance instead of partially modifying the install.

If Cassady is installed through a package manager in the future, prefer that package manager's update command instead of `cass update`.
