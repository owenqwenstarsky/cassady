use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct CodexAccessToken {
    value: String,
}

impl CodexAccessToken {
    pub fn new(value: String) -> Result<Self> {
        if value.trim().is_empty() {
            bail!("Codex access token is empty");
        }
        Ok(Self { value })
    }

    pub fn as_secret(&self) -> &str {
        &self.value
    }

    pub fn redacted(&self) -> &'static str {
        "<Codex access token>"
    }
}

impl std::fmt::Display for CodexAccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.redacted())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAuthStatus {
    pub path: PathBuf,
    pub auth_mode: Option<String>,
    pub has_access_token: bool,
    pub expires_at: Option<i64>,
    pub expired: bool,
    pub error: Option<String>,
}

impl CodexAuthStatus {
    pub fn is_usable(&self) -> bool {
        self.error.is_none() && self.has_access_token && !self.expired
    }

    pub fn recovery_hint(&self) -> &'static str {
        "Run `codex login` or sign in with the Codex app, then rerun `cass check`."
    }

    pub fn summary(&self) -> String {
        if self.is_usable() {
            if let Some(auth_mode) = &self.auth_mode {
                format!(
                    "{} contains an access token (auth mode: {auth_mode})",
                    pretty_path(&self.path)
                )
            } else {
                format!("{} contains an access token", pretty_path(&self.path))
            }
        } else if let Some(error) = &self.error {
            format!("{}: {error}", pretty_path(&self.path))
        } else if self.expired {
            format!(
                "{} contains an expired access token",
                pretty_path(&self.path)
            )
        } else {
            format!(
                "{} does not contain an access token",
                pretty_path(&self.path)
            )
        }
    }
}

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    auth_mode: Option<String>,
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    access_token: Option<String>,
}

pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

pub fn codex_auth_path() -> PathBuf {
    std::env::var_os("CODEX_AUTH_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home().join("auth.json"))
}

pub fn codex_config_path() -> PathBuf {
    std::env::var_os("CODEX_CONFIG_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home().join("config.toml"))
}

pub fn load_codex_access_token() -> Result<CodexAccessToken> {
    load_codex_access_token_from_path(&codex_auth_path())
}

pub fn load_codex_access_token_from_path(path: &Path) -> Result<CodexAccessToken> {
    let text = fs::read_to_string(path).with_context(|| {
        format!(
            "Codex auth not found at {}; run `codex login` or sign in with the Codex app",
            pretty_path(path)
        )
    })?;
    let parsed: CodexAuthFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing Codex auth at {}", pretty_path(path)))?;
    let token = parsed
        .tokens
        .and_then(|tokens| tokens.access_token)
        .filter(|token| !token.trim().is_empty())
        .with_context(|| {
            format!(
                "no access token found in {}; run `codex login` or sign in with the Codex app",
                pretty_path(path)
            )
        })?;
    if jwt_is_expired(&token) == Some(true) {
        bail!(
            "Codex access token in {} is expired; run `codex login` or sign in with the Codex app",
            pretty_path(path)
        );
    }
    CodexAccessToken::new(token)
}

pub fn check_codex_auth() -> CodexAuthStatus {
    check_codex_auth_at(&codex_auth_path())
}

pub fn check_codex_auth_at(path: &Path) -> CodexAuthStatus {
    let mut status = CodexAuthStatus {
        path: path.to_path_buf(),
        auth_mode: None,
        has_access_token: false,
        expires_at: None,
        expired: false,
        error: None,
    };

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            status.error = Some(format!("not readable ({err})"));
            return status;
        }
    };
    let parsed: CodexAuthFile = match serde_json::from_str(&text) {
        Ok(parsed) => parsed,
        Err(err) => {
            status.error = Some(format!("invalid JSON ({err})"));
            return status;
        }
    };
    status.auth_mode = parsed.auth_mode;
    let token = parsed
        .tokens
        .and_then(|tokens| tokens.access_token)
        .filter(|token| !token.trim().is_empty());
    if let Some(token) = token {
        status.has_access_token = true;
        status.expires_at = jwt_expiration(&token);
        status.expired = jwt_is_expired(&token).unwrap_or(false);
    }
    status
}

pub fn read_codex_default_model() -> Option<String> {
    read_codex_default_model_from_path(&codex_config_path())
}

pub fn read_codex_default_model_from_path(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.starts_with("model") {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "model" {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value)
            .trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn jwt_is_expired(token: &str) -> Option<bool> {
    let exp = jwt_expiration(token)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(exp <= now)
}

fn jwt_expiration(token: &str) -> Option<i64> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let bytes = base64_url_decode(payload).ok()?;
    let json: Value = serde_json::from_slice(&bytes).ok()?;
    json.get("exp")?.as_i64()
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut out = Vec::new();
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return Err("invalid base64 character".into()),
        } as u32;
        bits = (bits << 6) | value;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            out.push(((bits >> bit_count) & 0xff) as u8);
        }
    }
    Ok(out)
}

pub fn pretty_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reads_access_token_without_displaying_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"secret-token"}}"#,
        )
        .unwrap();

        let token = load_codex_access_token_from_path(&path).unwrap();
        assert_eq!(token.as_secret(), "secret-token");
        assert_eq!(token.to_string(), "<Codex access token>");
    }

    #[test]
    fn check_reports_missing_token_without_secret_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{"tokens":{"refresh_token":"refresh-secret","account_id":"acct"}}"#,
        )
        .unwrap();

        let status = check_codex_auth_at(&path);
        assert!(!status.is_usable());
        let summary = status.summary();
        assert!(!summary.contains("refresh-secret"));
        assert!(!summary.contains("acct"));
    }

    #[test]
    fn reads_model_from_codex_config() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "model = \"gpt-test\"\n").unwrap();

        assert_eq!(
            read_codex_default_model_from_path(&path).as_deref(),
            Some("gpt-test")
        );
    }
}
