use crate::cli::{Cli, DesktopArgs};
use anyhow::{anyhow, bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DESKTOP_BIN_ENV: &str = "CASSADY_DESKTOP_BIN";
const DESKTOP_FRONTEND_ASSET_MARKER: &[u8] = b"assets/index-";

pub fn launch(cli: &Cli, args: &DesktopArgs) -> Result<()> {
    let cwd = resolve_launch_cwd(cli.cwd.as_deref())?;
    let desktop_bin = locate_desktop_binary()?;

    if args.foreground {
        let status = Command::new(&desktop_bin)
            .current_dir(&cwd)
            .status()
            .with_context(|| format!("launching {}", desktop_bin.display()))?;
        if !status.success() {
            bail!("Cassady desktop exited with {status}");
        }
        return Ok(());
    }

    let pid = spawn_detached(&desktop_bin, &cwd)
        .with_context(|| format!("launching {} in the background", desktop_bin.display()))?;
    println!("Cassady desktop started in the background (pid {pid}). You can close this terminal.");
    Ok(())
}

fn resolve_launch_cwd(cwd: Option<&Path>) -> Result<PathBuf> {
    let cwd = match cwd {
        Some(path) => path.to_path_buf(),
        None => env::current_dir().context("resolving current directory")?,
    };
    cwd.canonicalize()
        .with_context(|| format!("resolving cwd {}", cwd.display()))
}

fn locate_desktop_binary() -> Result<PathBuf> {
    if let Some(path) = env::var_os(DESKTOP_BIN_ENV).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if is_runnable_file(&path) {
            validate_desktop_binary(&path)?;
            return Ok(path);
        }
        bail!(
            "{DESKTOP_BIN_ENV} points to {}, but that file does not exist or is not executable",
            path.display()
        );
    }

    let binary_name = desktop_binary_name();
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(binary_name);
            if is_runnable_file(&candidate) {
                validate_desktop_binary(&candidate)?;
                return Ok(candidate);
            }
        }
    }

    if let Some(path) = find_on_path(binary_name) {
        validate_desktop_binary(&path)?;
        return Ok(path);
    }

    Err(anyhow!(
        "could not find the Cassady desktop binary `{binary_name}`. Install or unpack it next to `cass`, put it on PATH, or set {DESKTOP_BIN_ENV}=/path/to/{binary_name}."
    ))
}

fn desktop_binary_name() -> &'static str {
    if cfg!(windows) {
        "cassady-desktop.exe"
    } else {
        "cassady-desktop"
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| is_runnable_file(candidate))
}

fn validate_desktop_binary(path: &Path) -> Result<()> {
    if has_embedded_frontend_assets(path)? {
        return Ok(());
    }

    bail!(
        "found Cassady desktop binary at {}, but it does not include the built frontend assets and would open a blank window. Rebuild it with `(cd cassady-desktop && cargo tauri build --no-bundle)` or install a macOS/Linux release/npm package that includes `cassady-desktop`.",
        path.display()
    );
}

fn has_embedded_frontend_assets(path: &Path) -> Result<bool> {
    let bytes = fs::read(path)
        .with_context(|| format!("checking embedded desktop assets in {}", path.display()))?;
    Ok(bytes
        .windows(DESKTOP_FRONTEND_ASSET_MARKER.len())
        .any(|window| window == DESKTOP_FRONTEND_ASSET_MARKER))
}

fn is_runnable_file(path: &Path) -> bool {
    path.is_file() && is_executable(path)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn spawn_detached(binary: &Path, cwd: &Path) -> Result<u32> {
    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach_command(&mut command);
    let child = command.spawn()?;
    Ok(child.id())
}

#[cfg(unix)]
fn detach_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn detach_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn detach_command(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_binary_name_has_platform_suffix() {
        let name = desktop_binary_name();
        if cfg!(windows) {
            assert_eq!(name, "cassady-desktop.exe");
        } else {
            assert_eq!(name, "cassady-desktop");
        }
    }

    #[test]
    fn find_on_path_returns_none_for_missing_binary() {
        let name = format!("definitely-not-cassady-desktop-{}", std::process::id());
        assert!(find_on_path(&name).is_none());
    }

    #[test]
    fn embedded_frontend_asset_marker_is_detected() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"prefix /assets/index-AbCdEf.js suffix").unwrap();
        assert!(has_embedded_frontend_assets(file.path()).unwrap());
    }

    #[test]
    fn missing_embedded_frontend_asset_marker_is_rejected() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"plain cargo-built desktop binary").unwrap();
        assert!(!has_embedded_frontend_assets(file.path()).unwrap());
    }
}
