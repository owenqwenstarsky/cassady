use anyhow::{bail, Context, Result};
use include_dir::{include_dir, Dir};
use std::fs;
use std::path::{Path, PathBuf};

static BUNDLED_DOCS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/docs");
const DOCS_HASH: &str = env!("CASS_DOCS_HASH");
const STAMP_FILE: &str = ".cass-docs-hash";

pub fn install(cass_root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(cass_root)
        .with_context(|| format!("creating Cass root {}", cass_root.display()))?;

    let dest = cass_root.join("docs");
    let stamp = dest.join(STAMP_FILE);
    let dest_is_managed_dir = match fs::symlink_metadata(&dest) {
        Ok(meta) => meta.is_dir(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => bail!("checking {}: {err}", dest.display()),
    };
    if dest_is_managed_dir
        && fs::read_to_string(&stamp)
            .map(|text| text.trim() == DOCS_HASH)
            .unwrap_or(false)
    {
        return Ok(dest);
    }

    let tmp = cass_root.join(format!(
        ".docs.tmp-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let backup = cass_root.join(format!(
        ".docs.backup-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    remove_path_if_exists(&tmp)?;
    remove_path_if_exists(&backup)?;

    if let Err(err) = extract_docs(&tmp) {
        let _ = remove_path_if_exists(&tmp);
        return Err(err);
    }

    if path_exists(&dest)? {
        fs::rename(&dest, &backup).with_context(|| {
            format!(
                "moving existing docs {} to {}",
                dest.display(),
                backup.display()
            )
        })?;
    }

    if let Err(err) = fs::rename(&tmp, &dest)
        .with_context(|| format!("installing bundled docs to {}", dest.display()))
    {
        let _ = remove_path_if_exists(&dest);
        if path_exists(&backup).unwrap_or(false) {
            let _ = fs::rename(&backup, &dest);
        }
        return Err(err);
    }

    remove_path_if_exists(&backup)?;
    Ok(dest)
}

pub fn docs_hash() -> &'static str {
    DOCS_HASH
}

fn extract_docs(tmp: &Path) -> Result<()> {
    fs::create_dir_all(tmp).with_context(|| format!("creating docs temp dir {}", tmp.display()))?;
    BUNDLED_DOCS
        .extract(tmp)
        .with_context(|| format!("extracting bundled docs to {}", tmp.display()))?;
    fs::write(tmp.join(STAMP_FILE), format!("{DOCS_HASH}\n"))
        .with_context(|| format!("writing docs stamp in {}", tmp.display()))?;
    Ok(())
}

fn path_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => bail!("checking {}: {err}", path.display()),
    }
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path)
            .with_context(|| format!("removing directory {}", path.display())),
        Ok(_) => fs::remove_file(path).with_context(|| format!("removing file {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => bail!("checking {}: {err}", path.display()),
    }
}
