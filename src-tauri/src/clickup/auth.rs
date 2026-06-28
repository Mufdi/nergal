//! ClickUp Personal API token storage.
//!
//! Primary store is the OS keyring (secret-service on Linux) under service
//! `nergal` / account `clickup-token`. When the keyring is unavailable the
//! token falls back to `~/.config/nergal/clickup.toml`, created atomically
//! at mode 0600 (temp file opened with the final mode + rename — no
//! write-then-chmod window), and the `on_disk` flag is surfaced so the UI
//! can disclose it.
//!
//! Leak guard: no function in this module may embed the token in an error
//! string or log line. TOML parse errors are redacted because toml's
//! diagnostics quote source snippets.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

const KEYRING_SERVICE: &str = "nergal";
const KEYRING_ACCOUNT: &str = "clickup-token";
const FALLBACK_FILE: &str = "clickup.toml";

// Manual Debug impls: a derived `{:?}` would print the raw token.
#[derive(Clone)]
pub struct StoredToken {
    pub token: String,
    /// True when the token lives in the plaintext fallback file.
    pub on_disk: bool,
}

impl std::fmt::Debug for StoredToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredToken")
            .field("token", &"[redacted]")
            .field("on_disk", &self.on_disk)
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FallbackFile {
    token: String,
}

impl std::fmt::Debug for FallbackFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackFile")
            .field("token", &"[redacted]")
            .finish()
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".config")
        })
        .join("nergal")
}

fn fallback_path() -> PathBuf {
    config_dir().join(FALLBACK_FILE)
}

fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| anyhow!("keyring entry init: {e}"))
}

/// Store the token; returns `true` when it landed in the on-disk fallback.
/// A keyring write also removes any stale fallback file so a cleared/rotated
/// token can't survive in plaintext.
pub fn store_token(token: &str) -> Result<bool> {
    let token = token.trim();
    if token.is_empty() {
        bail!("token is empty");
    }
    match keyring_entry().and_then(|e| {
        e.set_password(token)
            .map_err(|err| anyhow!("keyring write: {err}"))
    }) {
        Ok(()) => {
            remove_fallback_file()?;
            Ok(false)
        }
        Err(e) => {
            tracing::warn!("keyring unavailable ({e}); storing ClickUp token on disk at 0600");
            write_fallback_file(&fallback_path(), token)?;
            Ok(true)
        }
    }
}

/// Load the token: keyring first, fallback file second. `None` when neither
/// store has one.
pub fn load_token() -> Result<Option<StoredToken>> {
    // A transient keyring failure (D-Bus hiccup, locked collection) must not
    // masquerade as "no token" — that would flip the UI to unconfigured while
    // the token still sits in the keyring. Only a clean NoEntry plus a missing
    // fallback file means Ok(None); other keyring errors surface as Err so the
    // poller can retry instead of parking on no_token.
    let mut keyring_err: Option<anyhow::Error> = None;
    match keyring_entry() {
        Ok(entry) => match entry.get_password() {
            Ok(token) => {
                return Ok(Some(StoredToken {
                    token,
                    on_disk: false,
                }));
            }
            Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                tracing::warn!("keyring read failed ({e}); trying fallback file");
                keyring_err = Some(anyhow!("keyring read failed: {e}"));
            }
        },
        Err(e) => {
            tracing::warn!("keyring init failed ({e}); trying fallback file");
            keyring_err = Some(e);
        }
    }
    match read_fallback_file(&fallback_path())? {
        Some(stored) => Ok(Some(stored)),
        None => match keyring_err {
            Some(e) => Err(e),
            None => Ok(None),
        },
    }
}

/// Remove the token from both stores. Idempotent.
pub fn clear_token() -> Result<()> {
    if let Ok(entry) = keyring_entry() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => tracing::warn!("keyring delete failed: {e}"),
        }
    }
    remove_fallback_file()
}

fn remove_fallback_file() -> Result<()> {
    let path = fallback_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow!("removing {}: {e}", path.display())),
    }
}

fn write_fallback_file(path: &std::path::Path, token: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("token file path has no parent"))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating config dir {}", parent.display()))?;

    // Unique temp name per process; create_new guarantees we never open a
    // pre-existing (possibly wider-mode) file.
    let tmp = parent.join(format!(".{}.tmp-{}", FALLBACK_FILE, std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    // 0o600 on Unix; Windows has no POSIX mode bits — the per-user config dir +
    // Credential Manager (keyring) is the real boundary for this fallback file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp)
        .with_context(|| format!("creating token temp file in {}", parent.display()))?;

    let body = toml::to_string(&FallbackFile {
        token: token.to_string(),
    })
    .map_err(|_| anyhow!("serializing token file"))?;

    let write_result = file
        .write_all(body.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("writing token temp file: {e}"));
    }
    drop(file);

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("installing token file at {}: {e}", path.display()));
    }
    Ok(())
}

fn read_fallback_file(path: &std::path::Path) -> Result<Option<StoredToken>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("reading {}: {e}", path.display())),
    };
    // toml errors quote source snippets — never propagate them verbatim or
    // the token leaks into the error string.
    let parsed: FallbackFile = toml::from_str(&raw)
        .map_err(|_| anyhow!("malformed token file {} (redacted)", path.display()))?;
    Ok(Some(StoredToken {
        token: parsed.token,
        on_disk: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const TOKEN: &str = "pk_812345_SECRETSECRETSECRET";

    #[test]
    fn fallback_file_created_at_0600_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clickup.toml");

        write_fallback_file(&path, TOKEN).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let loaded = read_fallback_file(&path).unwrap().unwrap();
        assert_eq!(loaded.token, TOKEN);
        assert!(loaded.on_disk);
    }

    #[test]
    fn fallback_overwrite_keeps_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clickup.toml");
        write_fallback_file(&path, "first-token").unwrap();
        write_fallback_file(&path, TOKEN).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(read_fallback_file(&path).unwrap().unwrap().token, TOKEN);
        // No temp residue.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn fallback_write_error_never_contains_token() {
        let dir = tempfile::tempdir().unwrap();
        // Parent is a file, so create_dir_all fails.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("sub").join("clickup.toml");

        let err = write_fallback_file(&path, TOKEN).unwrap_err();
        assert!(!format!("{err:#}").contains(TOKEN));
    }

    #[test]
    fn malformed_fallback_error_never_contains_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clickup.toml");
        // Invalid TOML embedding the token: the parse error must redact it.
        std::fs::write(&path, format!("token = {TOKEN}")).unwrap();

        let err = read_fallback_file(&path).unwrap_err();
        assert!(!format!("{err:#}").contains(TOKEN));
    }

    #[test]
    fn missing_fallback_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            read_fallback_file(&dir.path().join("clickup.toml"))
                .unwrap()
                .is_none()
        );
    }
}
