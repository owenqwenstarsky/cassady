# v0.2.7 Self-Update Command Implementation Plan

## Goal

v0.2.7 adds a polished `cass update` command that can update Cassady from official GitHub releases without requiring users to manually download archives, verify checksums, unpack binaries, or rebuild from source.

Success statement:

> A user can run `cass update`, see the available release, choose the recommended prebuilt binary or a source build fallback, and finish with updated `cass` and `cassady` commands in the same install location.

## Scope

### In scope

- Add a `cass update` / `cassady update` subcommand.
- Query official Cassady GitHub releases from `owenqwenstarsky/cassady`.
- Compare the current binary version with the latest stable release.
- Download and install the matching prebuilt archive when available.
- Verify prebuilt archives with the shipped `.sha256` files before installing.
- Offer a source-build path that downloads release source for the selected tag and builds local binaries.
- Update both shipped binaries, `cass` and `cassady`, when possible.
- Use interactive prompts by default with clear summaries, confirmations, progress, success, and recovery messages.
- Provide non-interactive flags for check-only and yes-to-prompts usage.
- Keep `cass update` independent of model/provider setup so updates work even when `~/.cass` is missing or broken.
- Add tests for release parsing, target detection, asset selection, checksum validation, archive extraction safety, and install planning.
- Update README and bundled docs.

### Out of scope

- Publishing through Homebrew, apt, winget, Scoop, npm, or other package managers.
- Automatic background updates or prompts during normal chat startup.
- Updating Cassady when it was installed by an external package manager that should own the install directory.
- Privilege escalation, `sudo` automation, or administrator prompts.
- Code signing, notarization, or signature verification beyond existing SHA-256 files.
- Downgrading by default. Installing an older tag should require an explicit flag if supported.
- Cross-compiling in source mode. Source builds target the current host platform only.

## Context and Current State

Relevant files:

- `Cargo.toml`: package version and two binaries, `cass` and `cassady`.
- `src/cli.rs`: Clap command definitions currently include `check` and `setup`.
- `src/app.rs`: top-level command dispatch; update should run before setup/config loading.
- `src/main.rs` and `src/bin/cassady.rs`: both call `cassady::run()`.
- `README.md` and `docs/commands.md`: command documentation.
- `docs/platforms.md` and `docs/troubleshooting.md`: platform and recovery guidance.
- `AGENTS.md`: release artifacts use these names:
  - `cassady-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `cassady-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
  - `cassady-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
  - `cassady-vX.Y.Z-x86_64-pc-windows-gnu.zip`

Current releases include both `cass` and `cassady` in each archive plus one `.sha256` file per archive. The update command should reuse that release contract instead of inventing a new distribution format.

## Design Principles

1. **Boring and recoverable.** Updating should be explicit, easy to understand, and safe to interrupt before installation starts.
2. **Use official release artifacts first.** Prefer prebuilt archives with SHA-256 verification; fall back to source builds when the user asks or no asset matches.
3. **No surprise setup coupling.** Users should not need a configured provider, model, or API key to update the CLI.
4. **Respect install ownership.** Do not auto-escalate privileges or overwrite package-manager-owned paths without clear user confirmation.
5. **Both command names stay aligned.** If the user has both `cass` and `cassady` in the install directory, update them together.
6. **Interactive by default, scriptable when requested.** The normal path should be friendly; flags should support CI/check scripts.
7. **Fail closed on integrity.** Missing or mismatched checksums for prebuilt artifacts must stop installation.

## User Experience

### Default interactive flow

```text
$ cass update
Cassady update

Current version: v0.2.6
Latest release:  v0.2.7
Install path:    /usr/local/bin
Recommended:     prebuilt aarch64-apple-darwin archive

Update Cassady to v0.2.7? [Y/n]
```

If the user accepts, Cassady should show concise phases:

```text
Downloading cassady-v0.2.7-aarch64-apple-darwin.tar.gz ... 8.4 MB
Downloading cassady-v0.2.7-aarch64-apple-darwin.tar.gz.sha256 ... done
Verifying SHA-256 ... ok
Preparing cass and cassady ... ok
Installing to /usr/local/bin ... ok
Verifying installed version ... cass 0.2.7

Cassady is up to date.
```

If the current version is already latest:

```text
Cassady is already up to date.
Current version: v0.2.7
Latest release:  v0.2.7
```

### Prebuilt or source selection

The default `auto` mode should choose the prebuilt release asset when a supported target is detected. If no matching prebuilt exists, prompt for source mode:

```text
No prebuilt archive is available for this platform.
Build Cassady v0.2.7 from source instead? [Y/n]
```

If both paths are available and the user asks for source mode:

```sh
cass update --source
```

Cassady should confirm prerequisites before building:

```text
Source build requires cargo, rustc, and a working C toolchain.
Build Cassady v0.2.7 from release source now? [Y/n]
```

### Useful flags

Add a command shape like:

```sh
cass update [OPTIONS]
```

Suggested options:

- `--check`: check GitHub for the latest release and print status without installing.
- `--yes`: accept default prompts for non-interactive use.
- `--prebuilt`: require a matching prebuilt archive; fail instead of falling back to source.
- `--source`: build from release source even when a prebuilt archive exists.
- `--to TAG`: install a specific release tag such as `v0.2.7`.
- `--dry-run`: resolve the release, target, assets, and install path without downloading or installing.

Optional later flags, only if implementation needs them:

- `--stable-only`: ignore prerelease tags during latest-release selection if Cassady later publishes both stable and prerelease channels.
- `--install-dir PATH`: install into an explicit directory. This should be advanced and carefully documented because it can conflict with PATH order.

Avoid adding a public `--repo` override unless needed for testing; tests can inject a mock client instead.

## Design

### Module layout

Add a focused update module:

```rust
pub mod update;
```

Suggested internal types:

```rust
pub struct UpdateOptions { ... }
pub enum UpdateMode { Auto, Prebuilt, Source }
pub struct ReleaseInfo { ... }
pub struct ReleaseAsset { ... }
pub struct PlatformTarget { ... }
pub struct UpdatePlan { ... }
pub enum InstallAction { Replace, AddCompanion, SkipMissingCompanion }
```

`src/cli.rs` should add an `Update` subcommand with parsed flags. `src/app.rs` should dispatch it before setup/config loading:

```rust
if let Some(Command::Update(args)) = cli.command {
    return crate::update::run(args).await;
}
```

This keeps update usable even when `Config::load()` would fail.

### GitHub release discovery

Use the GitHub Releases API with an explicit user agent:

- Latest release: `GET https://api.github.com/repos/owenqwenstarsky/cassady/releases?per_page=30` and choose the highest semver non-draft tag, including prereleases because Cassady's current release process marks releases as prereleases.
- Specific tag: `GET https://api.github.com/repos/owenqwenstarsky/cassady/releases/tags/{tag}`

Parse:

- `tag_name`
- `name`
- `draft`
- `prerelease`
- `assets[].name`
- `assets[].browser_download_url`
- `assets[].size`
- `tarball_url` or `zipball_url` for source mode

Use `semver` to compare `env!("CARGO_PKG_VERSION")` with release tags after stripping a leading `v`. Draft releases should never be selected. Prereleases should be eligible by default while Cassady's official releases are marked as prereleases.

### Platform target mapping

Map the running platform to release asset targets:

| OS | Arch | Target | Archive |
| --- | --- | --- | --- |
| macOS | `aarch64` | `aarch64-apple-darwin` | `.tar.gz` |
| Linux | `x86_64` | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux | `aarch64` | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| Windows | `x86_64` | `x86_64-pc-windows-gnu` | `.zip` |

Unsupported platforms should produce a clean message and offer source mode when possible.

### Prebuilt update path

For tag `vX.Y.Z` and target `TARGET`, find:

```text
cassady-vX.Y.Z-TARGET.tar.gz
cassady-vX.Y.Z-TARGET.tar.gz.sha256
```

or on Windows:

```text
cassady-vX.Y.Z-x86_64-pc-windows-gnu.zip
cassady-vX.Y.Z-x86_64-pc-windows-gnu.zip.sha256
```

Flow:

1. Download archive and checksum into a temporary staging directory.
2. Parse the `.sha256` file and verify that the checksum filename matches the downloaded archive name.
3. Compute SHA-256 of the archive and compare exactly.
4. Extract into staging using path traversal checks.
5. Require the expected binaries:
   - Unix: `cass`, `cassady`
   - Windows: `cass.exe`, `cassady.exe`
6. Run the staged `cass --version` or `cassady --version` when possible and confirm the expected version.
7. Build an install plan for the current executable directory.
8. Confirm the final plan with the user unless `--yes` was supplied.
9. Replace binaries with backups and rollback on failure.
10. Verify installed version after replacement when possible.

Archive extraction must reject absolute paths, `..` components, symlinks that escape staging, and unexpected top-level layouts.

### Source-build update path

Source mode should still be tied to a GitHub release tag, not an arbitrary branch.

Flow:

1. Resolve the selected release tag.
2. Download release source from `tarball_url` or `zipball_url` into staging.
3. Extract with the same path traversal protections as prebuilt archives.
4. Verify `Cargo.toml` version matches the selected tag.
5. Run:

   ```sh
   cargo build --release --locked --bins
   ```

   from the extracted source tree.
6. Locate built binaries under `target/release/`.
7. Run staged `--version` checks.
8. Install using the same installer path as prebuilt updates.

Before source mode starts, check for `cargo` and `rustc` on PATH and show a clear error if they are missing. Do not attempt to install Rust automatically.

### Install planning and replacement

Determine the current executable path with `std::env::current_exe()`, then derive the install directory. The install plan should include:

- current binary path
- sibling `cass` path
- sibling `cassady` path
- which binaries currently exist
- which binaries are writable
- whether companion binaries will be updated, added, skipped, or blocked

Recommended behavior:

- Always update the currently running binary name.
- If the sibling binary exists in the same directory, update it too.
- If the sibling binary is missing and the directory is writable, ask whether to install it.
- If a target path is not writable, stop with an actionable message. Do not invoke `sudo` or administrator prompts automatically.
- Use backups such as `.cass-update-backup-v0.2.6-<timestamp>` during replacement.
- If any replacement fails, restore backups before returning an error.

Unix can generally replace a running executable via atomic rename. Windows cannot reliably overwrite the running `.exe`; implement one of these approaches during coding:

1. Preferred: stage replacements and spawn a small PowerShell or `cmd` helper that waits for the current process to exit, moves files into place, and writes a log.
2. Fallback: stage replacements and print exact manual copy commands if helper launch is unavailable.

Document any Windows limitation honestly in `docs/platforms.md` and `docs/troubleshooting.md`.

### Output and error style

Keep output concise and user-facing:

- Show current version, target version, install directory, selected mode, and asset/source name before changing files.
- Show clear phase lines for download, verify, build, install, and final verification.
- On failure, say whether anything was changed and where staging/backups are located.
- If update cannot proceed because the install path is not writable, tell the user which path failed and suggest reinstalling through the same method they originally used.

Avoid dumping raw GitHub JSON, backtraces, or Cargo logs unless the source build fails; in that case, preserve the final relevant Cargo output and staging path.

## Dependencies

Likely additions to `Cargo.toml`:

- `semver` for version comparison.
- `sha2` for SHA-256 verification.
- `tar` and `flate2` for `.tar.gz` extraction.
- `zip` for Windows release archives and GitHub source zips if used.

Prefer small, well-maintained crates. Reuse existing `reqwest`, `tokio`, `serde`, and `serde_json`.

## Implementation Steps

1. Add CLI parsing for `cass update` and dispatch it before setup/config loading.
2. Add `src/update.rs` with release API types, version comparison, and target detection.
3. Implement GitHub release fetching with a testable client abstraction or injectable base URL for tests.
4. Implement asset selection for current platform and update mode.
5. Implement download, progress reporting, and checksum verification for prebuilt archives.
6. Implement safe archive extraction and staged binary validation.
7. Implement install planning from `current_exe()` and companion binary detection.
8. Implement Unix replacement with backups and rollback.
9. Implement Windows staged-helper replacement or a clearly documented manual fallback.
10. Implement source mode: source download, version validation, prerequisite checks, `cargo build --release --locked --bins`, and staged binary validation.
11. Polish interactive prompts and `--check`, `--dry-run`, `--yes`, `--prebuilt`, `--source`, and `--to` behavior.
12. Update docs and release notes template expectations if needed.
13. Add tests and run full verification.

## Tests

Add focused unit tests for:

- parsing `vX.Y.Z` tags and comparing against the current version shape
- ignoring drafts and prereleases where applicable
- mapping supported and unsupported platform targets
- matching asset and checksum filenames
- parsing `.sha256` lines generated by the release process
- rejecting checksum filename mismatches and digest mismatches
- rejecting archive path traversal entries
- planning installation when only `cass`, only `cassady`, or both binaries exist
- refusing non-writable install targets in planning or dry-run mode
- source mode validating that `Cargo.toml` version matches the selected tag

Add integration-style tests with a mock HTTP server for:

- already-up-to-date response
- latest prebuilt update plan
- missing prebuilt with source fallback prompt path, where build execution can be mocked
- download checksum mismatch failure
- successful staged install into a temporary directory using fake binaries

Manual checks:

```sh
cargo fmt
cargo test --locked --all-targets
cargo run -- update --check
cargo run -- update --dry-run --to v0.2.7
```

For a real release candidate, test from a temporary install directory before using `cass update` on the developer's normal binary.

## Documentation

Update:

- `README.md`: mention `cass update` in install/update and everyday command sections.
- `docs/commands.md`: full command reference, flags, interactivity, examples, and exit behavior.
- `docs/platforms.md`: platform-specific update support and Windows replacement notes.
- `docs/troubleshooting.md`: network failures, checksum mismatch, no matching prebuilt, missing Rust toolchain, non-writable install directory, PATH conflicts, and rollback recovery.
- `docs/README.md`: add any new update-related links or summaries.

Document that users should prefer the package manager's update mechanism if Cassady was installed through a package manager in the future.

## Acceptance Criteria

- `cass update --check` reports the current/latest release without reading provider config.
- `cass update --dry-run` shows the selected release, mode, asset/source, and install plan without modifying files.
- On supported release targets, `cass update` can download the matching official archive, verify SHA-256, stage both binaries, and update the current install directory.
- `cass update --source` can download release source, build with `cargo build --release --locked --bins`, and install the resulting local binaries.
- Checksum mismatch, missing assets, unsupported platforms, missing Rust toolchain, and non-writable install paths fail with clear messages and no partial install.
- Existing `cass` and `cassady` sibling binaries remain version-aligned after a successful update.
- README and bundled docs explain the command accurately.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
