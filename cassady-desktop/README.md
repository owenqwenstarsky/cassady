# Cassady Desktop

Cassady Desktop is the v0.4.0 desktop preview. It uses the same Cassady config, providers, tools, approval policy, and `~/.cass/conversations/*.jsonl` sessions as the terminal CLI.

## Launch from a release archive

macOS and Linux release archives include three binaries:

- `cass`
- `cassady`
- `cassady-desktop`

Keep them in the same directory or put all three on `PATH`, then launch the desktop app with:

```sh
cass desktop
```

`cass desktop` starts `cassady-desktop` as a detached background process, so you can close the terminal after the window opens. For debugging, keep it attached with:

```sh
cass desktop --foreground
```

If the desktop binary is somewhere else, set:

```sh
CASSADY_DESKTOP_BIN=/path/to/cassady-desktop cass desktop
```

Run `cass setup` first if Cassady has no configured provider.

## Development

From the repository root:

```sh
cd cassady-desktop
npm install
cargo tauri build --target aarch64-apple-darwin
```

`cargo tauri build` runs the frontend build and embeds the generated assets into the standalone binary. A plain `cargo build -p cassady-desktop` is useful for Rust checks, but do not package that output because it may run in Tauri dev-asset mode and open to a blank window when copied elsewhere.

For a GUI dev session, run the Vite dev server and the Tauri app using your local Tauri workflow. Release packaging uses the raw `cassady-desktop` binary, not DMG, app bundle, AppImage, MSI, or other installer artifacts.
