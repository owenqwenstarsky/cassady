use crate::cli::UpdateArgs;
use anyhow::{anyhow, bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

const OWNER: &str = "owenqwenstarsky";
const REPO: &str = "cassady";
const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("cassady-updater/", env!("CARGO_PKG_VERSION"));
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateMode {
    Auto,
    Prebuilt,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformTarget {
    triple: &'static str,
    archive: ArchiveKind,
    binary_suffix: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
    tarball_url: Option<String>,
    zipball_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct SelectedAsset {
    archive: GitHubAsset,
    checksum: GitHubAsset,
    target: PlatformTarget,
}

#[derive(Debug, Clone)]
struct UpdateSelection {
    mode: UpdateMode,
    prebuilt: Option<SelectedAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallActionKind {
    Replace,
    AddCompanion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallAction {
    name: String,
    staged_path: PathBuf,
    target_path: PathBuf,
    kind: InstallActionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallPlan {
    current_exe: PathBuf,
    install_dir: PathBuf,
    actions: Vec<InstallAction>,
}

#[derive(Debug, Clone)]
struct StagedBinaries {
    cass: PathBuf,
    cassady: PathBuf,
    desktop: Option<PathBuf>,
}

pub async fn run(args: UpdateArgs) -> Result<()> {
    let mode = mode_from_args(&args);
    let current = current_version()?;
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;

    println!("Cassady update\n");

    let Some(release) = fetch_release(&client, args.to.as_deref()).await? else {
        println!("No published GitHub releases were found for {OWNER}/{REPO}.");
        return Ok(());
    };
    validate_release(&release)?;
    let target_version = parse_tag_version(&release.tag_name)?;

    println!("Current version: v{}", current);
    let prerelease_label = if release.prerelease {
        " (prerelease)"
    } else {
        ""
    };
    if let Some(name) = release.name.as_deref() {
        println!(
            "Latest release:  {}{} ({name})",
            release.tag_name, prerelease_label
        );
    } else {
        println!("Latest release:  {}{}", release.tag_name, prerelease_label);
    }

    if target_version < current {
        if args.to.is_some() {
            bail!(
                "selected release {} is older than the current version v{}; downgrades are not supported by cass update",
                release.tag_name,
                current
            );
        }
        println!("\nCurrent version is newer than the latest published release.");
        return Ok(());
    }

    if target_version == current {
        println!("\nCassady is already up to date.");
        return Ok(());
    }

    if args.check {
        println!("\nUpdate available: {}", release.tag_name);
        return Ok(());
    }

    let selection = select_update_path(&release, mode, args.yes, args.dry_run)?;
    let install_plan = plan_current_install(None, None)?;

    print_selection_summary(&release, &selection, &install_plan);

    if args.dry_run {
        println!("\nDry run only; no files were changed.");
        return Ok(());
    }

    confirm_or_bail(
        &format!("Update Cassady to {}?", release.tag_name),
        args.yes,
    )?;

    match selection.mode {
        UpdateMode::Prebuilt | UpdateMode::Auto => {
            let selected = selection
                .prebuilt
                .as_ref()
                .ok_or_else(|| anyhow!("no prebuilt update was selected"))?;
            run_prebuilt_update(&client, &release, selected, &install_plan).await?;
        }
        UpdateMode::Source => {
            run_source_update(&client, &release, &install_plan, args.yes).await?;
        }
    }

    println!("\nCassady is up to date.");
    Ok(())
}

fn mode_from_args(args: &UpdateArgs) -> UpdateMode {
    if args.prebuilt {
        UpdateMode::Prebuilt
    } else if args.source {
        UpdateMode::Source
    } else {
        UpdateMode::Auto
    }
}

async fn fetch_release(
    client: &reqwest::Client,
    tag: Option<&str>,
) -> Result<Option<GitHubRelease>> {
    if let Some(tag) = tag {
        let url = format!("{GITHUB_API}/repos/{OWNER}/{REPO}/releases/tags/{tag}");
        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("fetching release metadata from {url}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::NOT_FOUND {
                bail!("GitHub release tag {tag} was not found for {OWNER}/{REPO}");
            }
            bail!(
                "GitHub release request failed with {status}: {}",
                trim_for_error(&body)
            );
        }
        return response
            .json::<GitHubRelease>()
            .await
            .context("parsing GitHub release metadata")
            .map(Some);
    }

    // GitHub's /releases/latest endpoint ignores prereleases and returns 404 when
    // a project only publishes prereleases. Cassady's current release process uses
    // prereleases, so list releases and choose the highest semver non-draft tag.
    let url = format!("{GITHUB_API}/repos/{OWNER}/{REPO}/releases?per_page=30");
    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("fetching release metadata from {url}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!(
            "GitHub release request failed with {status}: {}",
            trim_for_error(&body)
        );
    }
    let releases = response
        .json::<Vec<GitHubRelease>>()
        .await
        .context("parsing GitHub release list")?;
    Ok(select_latest_release(releases))
}

fn select_latest_release(releases: Vec<GitHubRelease>) -> Option<GitHubRelease> {
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            parse_tag_version(&release.tag_name)
                .ok()
                .map(|version| (version, release))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, release)| release)
}

fn validate_release(release: &GitHubRelease) -> Result<()> {
    if release.draft {
        bail!("release {} is still a draft", release.tag_name);
    }
    Ok(())
}

fn current_version() -> Result<Version> {
    Version::parse(CURRENT_VERSION).context("parsing current Cassady version")
}

fn parse_tag_version(tag: &str) -> Result<Version> {
    let raw = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(raw).with_context(|| format!("parsing release tag {tag}"))
}

fn select_update_path(
    release: &GitHubRelease,
    mode: UpdateMode,
    assume_yes: bool,
    dry_run: bool,
) -> Result<UpdateSelection> {
    let platform = current_platform();
    let prebuilt = platform
        .as_ref()
        .and_then(|target| select_prebuilt_asset(release, target).transpose())
        .transpose()?;

    match mode {
        UpdateMode::Prebuilt => {
            let prebuilt = prebuilt.ok_or_else(|| {
                anyhow!(
                    "no matching prebuilt archive is available for this platform ({}/{})",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                )
            })?;
            Ok(UpdateSelection {
                mode: UpdateMode::Prebuilt,
                prebuilt: Some(prebuilt),
            })
        }
        UpdateMode::Source => Ok(UpdateSelection {
            mode: UpdateMode::Source,
            prebuilt,
        }),
        UpdateMode::Auto => {
            if let Some(prebuilt) = prebuilt {
                Ok(UpdateSelection {
                    mode: UpdateMode::Prebuilt,
                    prebuilt: Some(prebuilt),
                })
            } else {
                println!(
                    "\nNo prebuilt archive is available for this platform ({}/{}).",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                );
                if !dry_run {
                    confirm_or_bail("Build Cassady from release source instead?", assume_yes)?;
                }
                Ok(UpdateSelection {
                    mode: UpdateMode::Source,
                    prebuilt: None,
                })
            }
        }
    }
}

fn current_platform() -> Option<PlatformTarget> {
    platform_target(std::env::consts::OS, std::env::consts::ARCH)
}

fn platform_target(os: &str, arch: &str) -> Option<PlatformTarget> {
    match (os, arch) {
        ("macos", "aarch64") => Some(PlatformTarget {
            triple: "aarch64-apple-darwin",
            archive: ArchiveKind::TarGz,
            binary_suffix: "",
        }),
        ("linux", "x86_64") => Some(PlatformTarget {
            triple: "x86_64-unknown-linux-gnu",
            archive: ArchiveKind::TarGz,
            binary_suffix: "",
        }),
        ("linux", "aarch64") => Some(PlatformTarget {
            triple: "aarch64-unknown-linux-gnu",
            archive: ArchiveKind::TarGz,
            binary_suffix: "",
        }),
        ("windows", "x86_64") => Some(PlatformTarget {
            triple: "x86_64-pc-windows-gnu",
            archive: ArchiveKind::Zip,
            binary_suffix: ".exe",
        }),
        _ => None,
    }
}

fn select_prebuilt_asset(
    release: &GitHubRelease,
    target: &PlatformTarget,
) -> Result<Option<SelectedAsset>> {
    let ext = match target.archive {
        ArchiveKind::TarGz => ".tar.gz",
        ArchiveKind::Zip => ".zip",
    };
    let archive_name = format!("cassady-{}-{}{}", release.tag_name, target.triple, ext);
    let checksum_name = format!("{archive_name}.sha256");

    let archive = release
        .assets
        .iter()
        .find(|asset| asset.name == archive_name);
    let checksum = release
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name);

    match (archive, checksum) {
        (Some(archive), Some(checksum)) => Ok(Some(SelectedAsset {
            archive: archive.clone(),
            checksum: checksum.clone(),
            target: target.clone(),
        })),
        (None, _) => Ok(None),
        (Some(_), None) => {
            bail!("release asset {archive_name} is missing checksum file {checksum_name}")
        }
    }
}

fn print_selection_summary(
    release: &GitHubRelease,
    selection: &UpdateSelection,
    plan: &InstallPlan,
) {
    println!("Install path:    {}", plan.install_dir.display());
    match selection.mode {
        UpdateMode::Source => println!("Recommended:     source build from {}", release.tag_name),
        UpdateMode::Auto | UpdateMode::Prebuilt => {
            if let Some(prebuilt) = &selection.prebuilt {
                println!(
                    "Recommended:     prebuilt {} archive",
                    prebuilt.target.triple
                );
                println!("Asset:           {}", prebuilt.archive.name);
            }
        }
    }

    println!("\nInstall plan:");
    for action in &plan.actions {
        let label = match action.kind {
            InstallActionKind::Replace => "replace",
            InstallActionKind::AddCompanion => "add",
        };
        println!(
            "- {label} {} at {}",
            action.name,
            action.target_path.display()
        );
    }
}

async fn run_prebuilt_update(
    client: &reqwest::Client,
    release: &GitHubRelease,
    selected: &SelectedAsset,
    plan: &InstallPlan,
) -> Result<()> {
    let staging = create_staging_dir(&release.tag_name)?;
    println!("\nStaging update in {}", staging.display());

    let archive_path = staging.join(&selected.archive.name);
    let checksum_path = staging.join(&selected.checksum.name);

    download_asset(client, &selected.archive, &archive_path).await?;
    download_asset(client, &selected.checksum, &checksum_path).await?;

    print!("Verifying SHA-256 ... ");
    io::stdout().flush().ok();
    verify_checksum_file(&archive_path, &checksum_path)?;
    println!("ok");

    let extract_dir = staging.join("extract");
    fs::create_dir_all(&extract_dir)?;
    print!("Preparing cass and cassady ... ");
    io::stdout().flush().ok();
    match selected.target.archive {
        ArchiveKind::TarGz => safe_extract_tgz(&archive_path, &extract_dir)?,
        ArchiveKind::Zip => safe_extract_zip(&archive_path, &extract_dir)?,
    }
    let staged = find_staged_binaries(&extract_dir, selected.target.binary_suffix)?;
    validate_staged_version(&staged, &release.tag_name)?;
    println!("ok");

    let install_plan = plan_current_install(Some(&staged), Some(&plan.current_exe))?;
    install_staged_binaries(&install_plan)?;
    verify_installed_version(&install_plan, &release.tag_name)?;
    Ok(())
}

async fn run_source_update(
    client: &reqwest::Client,
    release: &GitHubRelease,
    plan: &InstallPlan,
    assume_yes: bool,
) -> Result<()> {
    confirm_or_bail(
        &format!(
            "Source build requires cargo, rustc, and a working C toolchain. Build Cassady {} from source now?",
            release.tag_name
        ),
        assume_yes,
    )?;
    ensure_command_available("cargo")?;
    ensure_command_available("rustc")?;

    let source_url = release
        .tarball_url
        .as_ref()
        .or(release.zipball_url.as_ref())
        .ok_or_else(|| {
            anyhow!(
                "release {} does not include a source archive URL",
                release.tag_name
            )
        })?;
    let is_zip = source_url.ends_with(".zip") || release.tarball_url.is_none();

    let staging = create_staging_dir(&release.tag_name)?;
    println!("\nStaging source build in {}", staging.display());
    let source_archive = staging.join(if is_zip {
        "source.zip"
    } else {
        "source.tar.gz"
    });
    download_url(client, source_url, &source_archive, "release source").await?;

    let source_extract = staging.join("source");
    fs::create_dir_all(&source_extract)?;
    print!("Extracting release source ... ");
    io::stdout().flush().ok();
    if is_zip {
        safe_extract_zip(&source_archive, &source_extract)?;
    } else {
        safe_extract_tgz(&source_archive, &source_extract)?;
    }
    println!("ok");

    let source_root = find_source_root(&source_extract)?;
    verify_source_version(&source_root.join("Cargo.toml"), &release.tag_name)?;

    println!("Building Cassady {} from source ...", release.tag_name);
    let output = tokio::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--locked")
        .arg("--bins")
        .current_dir(&source_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("running cargo build --release --locked --bins")?;
    if !output.status.success() {
        bail!(
            "source build failed; staging directory kept at {}\n{}",
            staging.display(),
            trim_for_error(&String::from_utf8_lossy(&output.stderr))
        );
    }

    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let built = StagedBinaries {
        cass: source_root
            .join("target")
            .join("release")
            .join(format!("cass{suffix}")),
        cassady: source_root
            .join("target")
            .join("release")
            .join(format!("cassady{suffix}")),
        desktop: None,
    };
    if !built.cass.is_file() || !built.cassady.is_file() {
        bail!("source build did not produce both cass and cassady binaries");
    }
    validate_staged_version(&built, &release.tag_name)?;

    let install_plan = plan_current_install(Some(&built), Some(&plan.current_exe))?;
    install_staged_binaries(&install_plan)?;
    verify_installed_version(&install_plan, &release.tag_name)?;
    Ok(())
}

async fn download_asset(client: &reqwest::Client, asset: &GitHubAsset, dest: &Path) -> Result<()> {
    let label = if asset.size > 0 {
        format!("{} ({})", asset.name, human_bytes(asset.size))
    } else {
        asset.name.clone()
    };
    download_url(client, &asset.browser_download_url, dest, &label).await
}

async fn download_url(client: &reqwest::Client, url: &str, dest: &Path, label: &str) -> Result<()> {
    print!("Downloading {label} ... ");
    io::stdout().flush().ok();
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("downloading {label}"))?;
    if !response.status().is_success() {
        bail!("download failed for {label}: HTTP {}", response.status());
    }
    let bytes = response.bytes().await?;
    fs::write(dest, &bytes).with_context(|| format!("writing {}", dest.display()))?;
    println!("{}", human_bytes(bytes.len() as u64));
    Ok(())
}

fn create_staging_dir(tag: &str) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dir =
        std::env::temp_dir().join(format!("cass-update-{tag}-{stamp}-{}", std::process::id()));
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating staging directory {}", dir.display()))?;
    Ok(dir)
}

fn parse_checksum_file(contents: &str) -> Result<(String, String)> {
    let line = contents
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| anyhow!("checksum file is empty"))?;
    let mut parts = line.split_whitespace();
    let digest = parts
        .next()
        .ok_or_else(|| anyhow!("checksum file is missing digest"))?;
    let filename = parts
        .next()
        .ok_or_else(|| anyhow!("checksum file is missing archive filename"))?;
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("checksum file contains an invalid SHA-256 digest");
    }
    Ok((digest.to_ascii_lowercase(), filename.to_string()))
}

fn verify_checksum_file(archive_path: &Path, checksum_path: &Path) -> Result<()> {
    let contents = fs::read_to_string(checksum_path)
        .with_context(|| format!("reading checksum file {}", checksum_path.display()))?;
    let (expected_digest, expected_filename) = parse_checksum_file(&contents)?;
    let archive_name = archive_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("archive path has no filename"))?;
    if expected_filename != archive_name {
        bail!("checksum file is for {expected_filename}, but downloaded archive is {archive_name}");
    }
    let actual_digest = sha256_file(archive_path)?;
    if actual_digest != expected_digest {
        bail!("SHA-256 mismatch for {archive_name}");
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn safe_extract_tgz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            bail!("archive contains a link entry, which is not allowed");
        }
        if !(kind.is_file() || kind.is_dir()) {
            continue;
        }
        let entry_path = entry.path()?.into_owned();
        let out = safe_join(dest, &entry_path)?;
        if kind.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&out)?;
        }
    }
    Ok(())
}

fn safe_extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| anyhow!("zip archive contains an unsafe path: {}", file.name()))?
            .to_path_buf();
        let out = safe_join(dest, &enclosed)?;
        if file.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out)?;
            io::copy(&mut file, &mut out_file)?;
            #[cfg(unix)]
            if let Some(mode) = file.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&out, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute() {
        bail!("archive contains an absolute path: {}", relative.display());
    }
    let mut out = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("archive path escapes destination: {}", relative.display())
            }
        }
    }
    Ok(out)
}

fn find_staged_binaries(root: &Path, suffix: &str) -> Result<StagedBinaries> {
    let cass_name = format!("cass{suffix}");
    let cassady_name = format!("cassady{suffix}");
    let cass = find_file_named(root, &cass_name)?
        .ok_or_else(|| anyhow!("archive does not contain {cass_name}"))?;
    let cassady = find_file_named(root, &cassady_name)?
        .ok_or_else(|| anyhow!("archive does not contain {cassady_name}"))?;
    let desktop = find_file_named(root, &format!("cassady-desktop{suffix}"))?;
    Ok(StagedBinaries {
        cass,
        cassady,
        desktop,
    })
}

fn find_file_named(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && entry.file_name() == OsStr::new(name) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn validate_staged_version(staged: &StagedBinaries, tag: &str) -> Result<()> {
    let expected = parse_tag_version(tag)?;
    let output = std::process::Command::new(&staged.cass)
        .arg("--version")
        .output()
        .with_context(|| format!("running staged binary {} --version", staged.cass.display()))?;
    if !output.status.success() {
        bail!("staged cass binary failed --version check");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(&expected.to_string()) {
        bail!(
            "staged cass binary reports unexpected version; expected {}, got {}",
            expected,
            stdout.trim()
        );
    }
    Ok(())
}

fn ensure_command_available(command: &str) -> Result<()> {
    let output = std::process::Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("checking for {command} on PATH"))?;
    if !output.success() {
        bail!("{command} is installed but did not run successfully");
    }
    Ok(())
}

fn find_source_root(root: &Path) -> Result<PathBuf> {
    let cargo = find_file_named(root, "Cargo.toml")?
        .ok_or_else(|| anyhow!("source archive does not contain Cargo.toml"))?;
    cargo
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("Cargo.toml has no parent directory"))
}

fn verify_source_version(cargo_toml: &Path, tag: &str) -> Result<()> {
    let expected = parse_tag_version(tag)?;
    let contents = fs::read_to_string(cargo_toml)
        .with_context(|| format!("reading {}", cargo_toml.display()))?;
    let actual = parse_package_version_from_cargo_toml(&contents)?;
    if actual != expected {
        bail!(
            "source Cargo.toml version {} does not match release {}",
            actual,
            tag
        );
    }
    Ok(())
}

fn parse_package_version_from_cargo_toml(contents: &str) -> Result<Version> {
    let mut in_package = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("version") {
            let Some((_, value)) = trimmed.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            return Version::parse(value).context("parsing package version from Cargo.toml");
        }
    }
    bail!("Cargo.toml is missing [package] version")
}

fn plan_current_install(
    staged: Option<&StagedBinaries>,
    current_exe_override: Option<&Path>,
) -> Result<InstallPlan> {
    let current_exe = current_exe_override
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_exe().context("resolving current executable path")?);
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?
        .to_path_buf();
    build_install_plan(&install_dir, &current_exe, staged)
}

fn build_install_plan(
    install_dir: &Path,
    current_exe: &Path,
    staged: Option<&StagedBinaries>,
) -> Result<InstallPlan> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let cass_target = install_dir.join(format!("cass{suffix}"));
    let cassady_target = install_dir.join(format!("cassady{suffix}"));
    let desktop_target = install_dir.join(format!("cassady-desktop{suffix}"));
    let current_name = current_exe
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("current executable path has no filename"))?;

    let mut action_inputs = vec![
        ("cass", cass_target, staged.map(|s| s.cass.clone())),
        ("cassady", cassady_target, staged.map(|s| s.cassady.clone())),
    ];
    if staged.and_then(|s| s.desktop.as_ref()).is_some() {
        action_inputs.push((
            "cassady-desktop",
            desktop_target,
            staged.and_then(|s| s.desktop.clone()),
        ));
    }

    let mut actions = Vec::new();
    for (name, target, staged_path) in action_inputs {
        let is_current = current_name
            == target
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
        let exists = target.exists();
        if !is_current && !exists {
            if staged_path.is_none() {
                continue;
            }
        }
        if !is_writable_target(&target)? {
            bail!("install target is not writable: {}", target.display());
        }
        let staged_path = staged_path.unwrap_or_else(|| PathBuf::from(format!("<staged {name}>")));
        let kind = if exists {
            InstallActionKind::Replace
        } else {
            InstallActionKind::AddCompanion
        };
        actions.push(InstallAction {
            name: name.to_string(),
            staged_path,
            target_path: target,
            kind,
        });
    }

    if actions.is_empty() {
        bail!("nothing to install for {}", current_exe.display());
    }

    Ok(InstallPlan {
        current_exe: current_exe.to_path_buf(),
        install_dir: install_dir.to_path_buf(),
        actions,
    })
}

fn is_writable_target(path: &Path) -> Result<bool> {
    if path.exists() {
        match OpenOptions::new().write(true).open(path) {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => Ok(false),
            Err(err) => {
                Err(err).with_context(|| format!("checking writability of {}", path.display()))
            }
        }
    } else {
        let Some(parent) = path.parent() else {
            return Ok(false);
        };
        Ok(!fs::metadata(parent)?.permissions().readonly())
    }
}

#[cfg_attr(windows, allow(unused_variables))]
fn install_staged_binaries(plan: &InstallPlan) -> Result<()> {
    #[cfg(windows)]
    {
        bail!(
            "automatic replacement of the running Windows executable is not available yet; staged files are ready but must be copied manually"
        );
    }

    #[cfg(not(windows))]
    {
        println!("Installing to {} ...", plan.install_dir.display());
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut installed: Vec<PathBuf> = Vec::new();

        let result = (|| -> Result<()> {
            for action in &plan.actions {
                if !action.staged_path.is_file() {
                    bail!("staged binary is missing: {}", action.staged_path.display());
                }
                if action.target_path.exists() {
                    let backup = action
                        .target_path
                        .with_extension(format!("cass-update-backup-v{CURRENT_VERSION}-{stamp}"));
                    fs::rename(&action.target_path, &backup).with_context(|| {
                        format!(
                            "backing up {} to {}",
                            action.target_path.display(),
                            backup.display()
                        )
                    })?;
                    backups.push((action.target_path.clone(), backup));
                }
                fs::copy(&action.staged_path, &action.target_path).with_context(|| {
                    format!(
                        "copying {} to {}",
                        action.staged_path.display(),
                        action.target_path.display()
                    )
                })?;
                installed.push(action.target_path.clone());
            }
            Ok(())
        })();

        if let Err(err) = result {
            for path in installed.iter().rev() {
                let _ = fs::remove_file(path);
            }
            for (target, backup) in backups.into_iter().rev() {
                let _ = fs::rename(backup, target);
            }
            return Err(err.context("install failed; attempted to restore backups"));
        }
        println!("Installing to {} ... ok", plan.install_dir.display());
        Ok(())
    }
}

fn verify_installed_version(plan: &InstallPlan, tag: &str) -> Result<()> {
    let expected = parse_tag_version(tag)?;
    let first = plan
        .actions
        .first()
        .ok_or_else(|| anyhow!("install plan had no actions"))?;
    let output = std::process::Command::new(&first.target_path)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", first.target_path.display()))?;
    if !output.status.success() {
        bail!("installed binary failed --version check");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(&expected.to_string()) {
        bail!(
            "installed binary reports unexpected version; expected {}, got {}",
            expected,
            stdout.trim()
        );
    }
    println!("Verifying installed version ... {}", stdout.trim());
    Ok(())
}

fn confirm_or_bail(prompt: &str, assume_yes: bool) -> Result<()> {
    if assume_yes {
        println!("{prompt} yes");
        return Ok(());
    }
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        bail!("{prompt} rerun with --yes to accept the default in a non-interactive context");
    }
    print!("{prompt} [Y/n] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    if answer.is_empty() || answer == "y" || answer == "yes" {
        Ok(())
    } else {
        bail!("update cancelled")
    }
}

fn trim_for_error(text: &str) -> String {
    let text = text.trim();
    const LIMIT: usize = 1200;
    if text.len() <= LIMIT {
        text.to_string()
    } else {
        format!("{}...", &text[..LIMIT])
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    fn release_with_assets(tag: &str, names: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_string(),
            name: Some(tag.to_string()),
            draft: false,
            prerelease: false,
            assets: names
                .iter()
                .map(|name| GitHubAsset {
                    name: name.to_string(),
                    browser_download_url: format!("https://example.test/{name}"),
                    size: 1,
                })
                .collect(),
            tarball_url: Some("https://example.test/source.tar.gz".to_string()),
            zipball_url: None,
        }
    }

    #[test]
    fn parses_tag_versions() {
        assert_eq!(
            parse_tag_version("v0.2.7").unwrap(),
            Version::parse("0.2.7").unwrap()
        );
        assert_eq!(
            parse_tag_version("0.2.7").unwrap(),
            Version::parse("0.2.7").unwrap()
        );
    }

    #[test]
    fn validates_release_flags() {
        let mut release = release_with_assets("v0.2.7", &[]);
        release.draft = true;
        assert!(validate_release(&release).is_err());
        release.draft = false;
        release.prerelease = true;
        assert!(validate_release(&release).is_ok());
    }

    #[test]
    fn selects_latest_non_draft_release_including_prereleases() {
        let mut v026 = release_with_assets("v0.2.6", &[]);
        v026.prerelease = true;
        let mut v027 = release_with_assets("v0.2.7", &[]);
        v027.prerelease = true;
        let mut v999 = release_with_assets("v9.9.9", &[]);
        v999.draft = true;
        let selected = select_latest_release(vec![v026, v999, v027]).unwrap();
        assert_eq!(selected.tag_name, "v0.2.7");
        assert!(selected.prerelease);
    }

    #[test]
    fn maps_supported_platform_targets() {
        assert_eq!(
            platform_target("macos", "aarch64").unwrap().triple,
            "aarch64-apple-darwin"
        );
        assert_eq!(
            platform_target("linux", "x86_64").unwrap().triple,
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            platform_target("linux", "aarch64").unwrap().triple,
            "aarch64-unknown-linux-gnu"
        );
        assert_eq!(
            platform_target("windows", "x86_64").unwrap().triple,
            "x86_64-pc-windows-gnu"
        );
        assert!(platform_target("macos", "x86_64").is_none());
    }

    #[test]
    fn selects_prebuilt_assets_with_checksums() {
        let release = release_with_assets(
            "v0.2.7",
            &[
                "cassady-v0.2.7-x86_64-unknown-linux-gnu.tar.gz",
                "cassady-v0.2.7-x86_64-unknown-linux-gnu.tar.gz.sha256",
            ],
        );
        let target = platform_target("linux", "x86_64").unwrap();
        let selected = select_prebuilt_asset(&release, &target).unwrap().unwrap();
        assert_eq!(
            selected.archive.name,
            "cassady-v0.2.7-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn errors_when_prebuilt_checksum_missing() {
        let release = release_with_assets(
            "v0.2.7",
            &["cassady-v0.2.7-x86_64-unknown-linux-gnu.tar.gz"],
        );
        let target = platform_target("linux", "x86_64").unwrap();
        assert!(select_prebuilt_asset(&release, &target).is_err());
    }

    #[test]
    fn parses_sha256_files() {
        let (digest, filename) = parse_checksum_file(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  cassady.tar.gz\n",
        )
        .unwrap();
        assert_eq!(
            digest,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(filename, "cassady.tar.gz");
    }

    #[test]
    fn verifies_sha256_file_and_filename() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("cassady.tar.gz");
        fs::write(&archive, b"hello").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"hello"));
        let checksum = dir.path().join("cassady.tar.gz.sha256");
        fs::write(&checksum, format!("{digest}  cassady.tar.gz\n")).unwrap();
        verify_checksum_file(&archive, &checksum).unwrap();
        fs::write(&checksum, format!("{digest}  other.tar.gz\n")).unwrap();
        assert!(verify_checksum_file(&archive, &checksum).is_err());
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let root = Path::new("/tmp/root");
        assert!(safe_join(root, Path::new("cassady/cass")).is_ok());
        assert!(safe_join(root, Path::new("../cass")).is_err());
        assert!(safe_join(root, Path::new("/tmp/cass")).is_err());
    }

    #[test]
    fn tgz_extraction_rejects_link_entries() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("bad.tar.gz");
        {
            let file = File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_link_name("/tmp/escape").unwrap();
            header.set_cksum();
            tar.append_data(&mut header, "link", &b""[..]).unwrap();
            tar.finish().unwrap();
        }
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        assert!(safe_extract_tgz(&archive_path, &out).is_err());
    }

    #[test]
    fn parses_package_version_from_cargo_toml() {
        let version = parse_package_version_from_cargo_toml(
            "[package]\nname = \"cassady\"\nversion = \"0.2.7\"\n\n[dependencies]\n",
        )
        .unwrap();
        assert_eq!(version, Version::parse("0.2.7").unwrap());
    }

    #[test]
    fn plans_install_for_existing_and_missing_companion() {
        let dir = tempfile::tempdir().unwrap();
        let cass = dir
            .path()
            .join(if cfg!(windows) { "cass.exe" } else { "cass" });
        fs::write(&cass, b"old").unwrap();
        let staged_dir = tempfile::tempdir().unwrap();
        let staged = StagedBinaries {
            cass: staged_dir
                .path()
                .join(if cfg!(windows) { "cass.exe" } else { "cass" }),
            cassady: staged_dir.path().join(if cfg!(windows) {
                "cassady.exe"
            } else {
                "cassady"
            }),
            desktop: None,
        };
        fs::write(&staged.cass, b"new").unwrap();
        fs::write(&staged.cassady, b"new").unwrap();
        let plan = build_install_plan(dir.path(), &cass, Some(&staged)).unwrap();
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.actions[0].kind, InstallActionKind::Replace);
        assert_eq!(plan.actions[1].kind, InstallActionKind::AddCompanion);
    }

    #[test]
    fn plans_install_for_staged_desktop_binary() {
        let dir = tempfile::tempdir().unwrap();
        let cass = dir
            .path()
            .join(if cfg!(windows) { "cass.exe" } else { "cass" });
        fs::write(&cass, b"old").unwrap();
        let staged_dir = tempfile::tempdir().unwrap();
        let desktop_name = if cfg!(windows) {
            "cassady-desktop.exe"
        } else {
            "cassady-desktop"
        };
        let staged = StagedBinaries {
            cass: staged_dir
                .path()
                .join(if cfg!(windows) { "cass.exe" } else { "cass" }),
            cassady: staged_dir.path().join(if cfg!(windows) {
                "cassady.exe"
            } else {
                "cassady"
            }),
            desktop: Some(staged_dir.path().join(desktop_name)),
        };
        fs::write(&staged.cass, b"new").unwrap();
        fs::write(&staged.cassady, b"new").unwrap();
        fs::write(staged.desktop.as_ref().unwrap(), b"desktop").unwrap();

        let plan = build_install_plan(dir.path(), &cass, Some(&staged)).unwrap();
        assert_eq!(plan.actions.len(), 3);
        assert_eq!(plan.actions[2].name, "cassady-desktop");
        assert_eq!(plan.actions[2].kind, InstallActionKind::AddCompanion);
    }

    #[test]
    #[cfg(not(windows))]
    fn installs_staged_binaries_with_backup() {
        let dir = tempfile::tempdir().unwrap();
        let cass = dir.path().join("cass");
        fs::write(&cass, b"old cass").unwrap();
        let cassady = dir.path().join("cassady");
        fs::write(&cassady, b"old cassady").unwrap();

        let staged_dir = tempfile::tempdir().unwrap();
        let staged = StagedBinaries {
            cass: staged_dir.path().join("cass"),
            cassady: staged_dir.path().join("cassady"),
            desktop: None,
        };
        fs::write(&staged.cass, b"new cass").unwrap();
        fs::write(&staged.cassady, b"new cassady").unwrap();

        let plan = build_install_plan(dir.path(), &cass, Some(&staged)).unwrap();
        install_staged_binaries(&plan).unwrap();
        assert_eq!(fs::read(&cass).unwrap(), b"new cass");
        assert_eq!(fs::read(&cassady).unwrap(), b"new cassady");
    }
}
