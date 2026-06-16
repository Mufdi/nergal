//! Linear Personal API key storage + the OAuth-extensible auth header.
//!
//! Primary store is the OS keyring (secret-service on Linux) under service
//! `cluihud` / account `linear-token`. When the keyring is unavailable the key
//! falls back to `~/.config/cluihud/linear.toml`, created atomically at mode
//! 0600 (temp file opened with the final mode + rename — no write-then-chmod
//! window), and the `on_disk` flag is surfaced so the UI can disclose it.
//!
//! Leak guard: no function here may embed the key in an error string or log
//! line. TOML parse errors are redacted because toml's diagnostics quote
//! source snippets.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

const KEYRING_SERVICE: &str = "cluihud";
const KEYRING_ACCOUNT: &str = "linear-token";
const FALLBACK_FILE: &str = "linear.toml";

/// How the authorization header is built. Personal keys send the raw key;
/// OAuth (deferred to a future `linear-oauth` change) sends `Bearer <token>`.
/// The enum is the seam that lets OAuth be added without reworking the client
/// or this module — only the variant constructed changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    Personal,
    /// Reserved: not constructed by this change. Present so the header builder
    /// already branches and a later OAuth change is purely additive.
    OAuthBearer,
}

/// Build the `Authorization` header value for a secret under the given mode.
/// Personal keys carry NO `Bearer` prefix (Linear's documented format);
/// OAuth tokens do.
pub fn authorization_header_value(mode: AuthMode, secret: &str) -> String {
    match mode {
        AuthMode::Personal => secret.to_string(),
        AuthMode::OAuthBearer => format!("Bearer {secret}"),
    }
}

// Manual Debug impls: a derived `{:?}` would print the raw key.
#[derive(Clone)]
pub struct StoredKey {
    pub key: String,
    /// True when the key lives in the plaintext fallback file.
    pub on_disk: bool,
}

impl std::fmt::Debug for StoredKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredKey")
            .field("key", &"[redacted]")
            .field("on_disk", &self.on_disk)
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FallbackFile {
    key: String,
}

impl std::fmt::Debug for FallbackFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackFile")
            .field("key", &"[redacted]")
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
        .join("cluihud")
}

fn fallback_path() -> PathBuf {
    config_dir().join(FALLBACK_FILE)
}

fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| anyhow!("keyring entry init: {e}"))
}

/// Store the key; returns `true` when it landed in the on-disk fallback. A
/// keyring write also removes any stale fallback file so a cleared/rotated key
/// can't survive in plaintext.
pub fn store_key(key: &str) -> Result<bool> {
    let key = key.trim();
    if key.is_empty() {
        bail!("key is empty");
    }
    match keyring_entry().and_then(|e| {
        e.set_password(key)
            .map_err(|err| anyhow!("keyring write: {err}"))
    }) {
        Ok(()) => {
            remove_fallback_file()?;
            Ok(false)
        }
        Err(e) => {
            tracing::warn!("keyring unavailable ({e}); storing Linear key on disk at 0600");
            write_fallback_file(&fallback_path(), key)?;
            Ok(true)
        }
    }
}

/// Load the key: keyring first, fallback file second. `None` when neither
/// store has one.
pub fn load_key() -> Result<Option<StoredKey>> {
    // A transient keyring failure (D-Bus hiccup, locked collection) must not
    // masquerade as "no key" — that would flip the UI to unconfigured while the
    // key still sits in the keyring. Only a clean NoEntry plus a missing
    // fallback file means Ok(None); other keyring errors surface as Err so the
    // poller retries instead of parking on no_key.
    let mut keyring_err: Option<anyhow::Error> = None;
    match keyring_entry() {
        Ok(entry) => match entry.get_password() {
            Ok(key) => {
                return Ok(Some(StoredKey {
                    key,
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

/// Remove the key from both stores. Idempotent.
pub fn clear_key() -> Result<()> {
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

fn write_fallback_file(path: &std::path::Path, key: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("key file path has no parent"))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating config dir {}", parent.display()))?;

    // Unique temp name per process; create_new guarantees we never open a
    // pre-existing (possibly wider-mode) file.
    let tmp = parent.join(format!(".{}.tmp-{}", FALLBACK_FILE, std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| format!("creating key temp file in {}", parent.display()))?;

    let body = toml::to_string(&FallbackFile {
        key: key.to_string(),
    })
    .map_err(|_| anyhow!("serializing key file"))?;

    let write_result = file
        .write_all(body.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("writing key temp file: {e}"));
    }
    drop(file);

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("installing key file at {}: {e}", path.display()));
    }
    Ok(())
}

fn read_fallback_file(path: &std::path::Path) -> Result<Option<StoredKey>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("reading {}: {e}", path.display())),
    };
    // toml errors quote source snippets — never propagate them verbatim or the
    // key leaks into the error string.
    let parsed: FallbackFile = toml::from_str(&raw)
        .map_err(|_| anyhow!("malformed key file {} (redacted)", path.display()))?;
    Ok(Some(StoredKey {
        key: parsed.key,
        on_disk: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const KEY: &str = "lin_api_SECRETSECRETSECRET";

    #[test]
    fn personal_header_has_no_bearer_prefix() {
        assert_eq!(authorization_header_value(AuthMode::Personal, KEY), KEY);
    }

    #[test]
    fn oauth_header_has_bearer_prefix() {
        assert_eq!(
            authorization_header_value(AuthMode::OAuthBearer, "tok"),
            "Bearer tok"
        );
    }

    #[test]
    fn fallback_file_created_at_0600_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.toml");

        write_fallback_file(&path, KEY).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let loaded = read_fallback_file(&path).unwrap().unwrap();
        assert_eq!(loaded.key, KEY);
        assert!(loaded.on_disk);
    }

    #[test]
    fn fallback_overwrite_keeps_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.toml");
        write_fallback_file(&path, "first-key").unwrap();
        write_fallback_file(&path, KEY).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(read_fallback_file(&path).unwrap().unwrap().key, KEY);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn fallback_write_error_never_contains_key() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("sub").join("linear.toml");

        let err = write_fallback_file(&path, KEY).unwrap_err();
        assert!(!format!("{err:#}").contains(KEY));
    }

    #[test]
    fn malformed_fallback_error_never_contains_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.toml");
        std::fs::write(&path, format!("key = {KEY}")).unwrap();

        let err = read_fallback_file(&path).unwrap_err();
        assert!(!format!("{err:#}").contains(KEY));
    }

    #[test]
    fn missing_fallback_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            read_fallback_file(&dir.path().join("linear.toml"))
                .unwrap()
                .is_none()
        );
    }
}
