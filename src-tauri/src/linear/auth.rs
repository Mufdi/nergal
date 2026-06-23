//! Linear Personal API key storage + the OAuth-extensible auth header.
//!
//! Primary store is the OS keyring (secret-service on Linux) under service
//! `nergal` / account `linear-token`. When the keyring is unavailable the key
//! falls back to `~/.config/nergal/linear.toml`, created atomically at mode
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

const KEYRING_SERVICE: &str = "nergal";
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
        .join("nergal")
}

fn fallback_path() -> PathBuf {
    config_dir().join(FALLBACK_FILE)
}

/// Defense in depth: `org_id` comes from the Linear API (a UUID) and is
/// interpolated into a keyring account string AND a filename. Reject anything
/// outside `[A-Za-z0-9-]` so a non-UUID value can never path-traverse out of the
/// config dir or collide the `::`-delimited account namespace.
fn validate_org_id(org_id: &str) -> Result<()> {
    if !org_id.is_empty()
        && org_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Ok(())
    } else {
        bail!("invalid org id");
    }
}

/// Per-workspace keyring account: `linear-token::<org_id>`. The bare
/// `linear-token` account is the legacy single-key store, migrated on first run.
fn account_for(org_id: &str) -> String {
    format!("{KEYRING_ACCOUNT}::{org_id}")
}

/// Per-workspace 0600 fallback file when the keyring is unavailable.
fn fallback_path_for(org_id: &str) -> PathBuf {
    // org_id is a Linear UUID (no path separators); safe as a filename component.
    config_dir().join(format!("linear-{org_id}.toml"))
}

fn keyring_entry_for(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| anyhow!("keyring entry init: {e}"))
}

/// Store a key under a specific keyring account + fallback path. Returns `true`
/// when it landed in the on-disk fallback. A keyring write removes any stale
/// fallback so a rotated key can't survive in plaintext.
fn store_to(account: &str, fallback: &std::path::Path, key: &str) -> Result<bool> {
    let key = key.trim();
    if key.is_empty() {
        bail!("key is empty");
    }
    match keyring_entry_for(account).and_then(|e| {
        e.set_password(key)
            .map_err(|err| anyhow!("keyring write: {err}"))
    }) {
        Ok(()) => {
            remove_fallback_at(fallback)?;
            Ok(false)
        }
        Err(e) => {
            tracing::warn!("keyring unavailable ({e}); storing Linear key on disk at 0600");
            write_fallback_file(fallback, key)?;
            Ok(true)
        }
    }
}

fn load_from(account: &str, fallback: &std::path::Path) -> Result<Option<StoredKey>> {
    let mut keyring_err: Option<anyhow::Error> = None;
    match keyring_entry_for(account) {
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
    match read_fallback_file(fallback)? {
        Some(stored) => Ok(Some(stored)),
        None => match keyring_err {
            Some(e) => Err(e),
            None => Ok(None),
        },
    }
}

fn remove_from(account: &str, fallback: &std::path::Path) -> Result<()> {
    if let Ok(entry) = keyring_entry_for(account) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => tracing::warn!("keyring delete failed: {e}"),
        }
    }
    remove_fallback_at(fallback)
}

/// Store the active-workspace key (per-org account). Returns `true` if on-disk.
pub fn store_key_for(org_id: &str, key: &str) -> Result<bool> {
    validate_org_id(org_id)?;
    store_to(&account_for(org_id), &fallback_path_for(org_id), key)
}

/// Load a workspace's key (per-org account). `None` when neither store has one.
pub fn load_key_for(org_id: &str) -> Result<Option<StoredKey>> {
    validate_org_id(org_id)?;
    load_from(&account_for(org_id), &fallback_path_for(org_id))
}

/// Remove a workspace's key from both stores. Idempotent.
pub fn remove_key_for(org_id: &str) -> Result<()> {
    validate_org_id(org_id)?;
    remove_from(&account_for(org_id), &fallback_path_for(org_id))
}

/// Store the legacy single key; returns `true` when it landed on disk. Kept so
/// the legacy `linear_set_key` path still works (the next poll migrates it to a
/// per-workspace entry).
pub fn store_key(key: &str) -> Result<bool> {
    store_to(KEYRING_ACCOUNT, &fallback_path(), key)
}

/// Load the legacy single key (the pre-multi-workspace store). A transient
/// keyring failure surfaces as Err (not Ok(None)) so the migration retries
/// instead of treating it as "no legacy key". Used only by the one-time
/// migration.
pub fn load_key() -> Result<Option<StoredKey>> {
    load_from(KEYRING_ACCOUNT, &fallback_path())
}

/// Remove the legacy key from both stores. Idempotent. Called after the legacy
/// key has been migrated into a per-workspace entry.
pub fn clear_key() -> Result<()> {
    remove_from(KEYRING_ACCOUNT, &fallback_path())
}

fn remove_fallback_at(path: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(path) {
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

    // Unique temp name per target file + process; create_new guarantees we
    // never open a pre-existing (possibly wider-mode) file.
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(FALLBACK_FILE);
    let tmp = parent.join(format!(".{fname}.tmp-{}", std::process::id()));
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

    #[test]
    fn validate_org_id_rejects_path_traversal_and_injection() {
        assert!(validate_org_id("3752ff73-ed03-4477-8946-2a43f862de6e").is_ok());
        assert!(validate_org_id("../../etc/passwd").is_err());
        assert!(validate_org_id("a/b").is_err());
        assert!(validate_org_id("a::b").is_err());
        assert!(validate_org_id("").is_err());
        // The per-org key functions reject before touching keyring/disk.
        assert!(load_key_for("../evil").is_err());
        assert!(remove_key_for("a/b").is_err());
    }
}
