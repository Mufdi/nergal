//! Shared keyring + 0600-fallback-file credential store used by both the
//! ClickUp and Linear auth modules.
//!
//! Primary store is the OS keyring (secret-service on Linux) under a
//! `(service, account)` pair. When the keyring is unavailable the secret
//! falls back to a plaintext file under `~/.config/nergal/`, created
//! atomically at mode 0600 (temp file opened with the final mode + rename —
//! no write-then-chmod window), and the `on_disk` flag is surfaced so the UI
//! can disclose it.
//!
//! `CredentialStore` is built against Linear's parametrized `store_to`/
//! `load_from`/`remove_from` shape (design D3) so it also serves Linear's
//! per-workspace accounts/files; ClickUp's flat single-account wrapper
//! constructs one instance with its fixed constants. Tracker-specific
//! concerns — Linear's `AuthMode`/OAuth header, its `validate_org_id`
//! path-traversal guard, and the per-org `linear-token::{org_id}` namespacing
//! — stay OUTSIDE this struct as wrappers around it (design D3): a third
//! tracker without multi-workspace support would otherwise inherit a dead
//! field.
//!
//! **On-disk field name (design Revision 1, MAJOR).** ClickUp's legacy TOML
//! key is `token`, Linear's is `key` — both literal keys already on disk at
//! `~/.config/nergal/{clickup,linear}.toml`. The shared fallback struct names
//! its field `secret` with `#[serde(alias = "token", alias = "key")]` so both
//! trackers' existing files still deserialize after upgrade; new writes
//! converge on `secret`. Downgrading past this point re-prompts for auth on
//! the file-fallback path only (keyring path unaffected) — accepted as a
//! forward-only-updater non-goal (design Revision 2).
//!
//! **Keyring constant pass-through (design Revision 2, MINOR — bigger blast
//! radius than the file fallback).** Callers MUST construct this with the
//! pre-refactor `(service, account, filename)` literals unchanged
//! (`clickup-token`/`clickup.toml`, `linear-token`/`linear.toml`) — "tidying"
//! an account name here makes an existing user's stored secret invisible on
//! the keyring path, forcing a silent re-auth.
//!
//! Leak guard: no function in this module may embed the secret in an error
//! string or log line. TOML parse errors are redacted because toml's
//! diagnostics quote source snippets.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

// Manual Debug impl: a derived `{:?}` would print the raw secret.
#[derive(Clone)]
pub struct StoredSecret {
    pub secret: String,
    /// True when the secret lives in the plaintext fallback file.
    pub on_disk: bool,
}

impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSecret")
            .field("secret", &"[redacted]")
            .field("on_disk", &self.on_disk)
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FallbackFile {
    #[serde(alias = "token", alias = "key")]
    secret: String,
}

impl std::fmt::Debug for FallbackFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackFile")
            .field("secret", &"[redacted]")
            .finish()
    }
}

/// Directory for the plaintext fallback files. On Windows this is the NON-roaming
/// local app-data dir (`%LOCALAPPDATA%`) so a plaintext secret is never synced to
/// an AD roaming-profile share; on other platforms the user config dir. Keyring
/// (Credential Manager on Windows) stays the primary store — these files only
/// appear when the keyring is unavailable.
fn fallback_dir() -> PathBuf {
    #[cfg(windows)]
    let base = dirs::data_local_dir();
    #[cfg(not(windows))]
    let base = dirs::config_dir();
    base.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".config")
    })
    .join("nergal")
}

/// A tracker's keyring + fallback-file credential, parametrized by service,
/// account, and fallback filename.
pub struct CredentialStore {
    service: &'static str,
    account: String,
    filename: String,
}

impl CredentialStore {
    pub fn new(
        service: &'static str,
        account: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        Self {
            service,
            account: account.into(),
            filename: filename.into(),
        }
    }

    fn fallback_path(&self) -> PathBuf {
        fallback_dir().join(&self.filename)
    }

    /// Pre-hardening roaming location (`%APPDATA%\nergal\<filename>`). Read +
    /// cleaned on Windows so a secret written by an older build migrates to the
    /// local dir instead of being lost — and its roaming plaintext copy is removed.
    #[cfg(windows)]
    fn legacy_fallback_path(&self) -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("nergal").join(&self.filename))
    }

    fn keyring_entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(self.service, &self.account)
            .map_err(|e| anyhow!("keyring entry init: {e}"))
    }

    /// Store the secret; returns `true` when it landed in the on-disk fallback.
    /// A keyring write also removes any stale fallback file so a cleared/rotated
    /// secret can't survive in plaintext.
    pub fn store(&self, secret: &str) -> Result<bool> {
        let secret = secret.trim();
        if secret.is_empty() {
            bail!("secret is empty");
        }
        match self.keyring_entry().and_then(|e| {
            e.set_password(secret)
                .map_err(|err| anyhow!("keyring write: {err}"))
        }) {
            Ok(()) => {
                self.remove_fallback_file()?;
                Ok(false)
            }
            Err(e) => {
                tracing::warn!(
                    "keyring unavailable ({e}); storing {} secret on disk at 0600",
                    self.account
                );
                write_fallback_file(&self.fallback_path(), secret)?;
                Ok(true)
            }
        }
    }

    /// Load the secret: keyring first, fallback file second. `None` when neither
    /// store has one.
    pub fn load(&self) -> Result<Option<StoredSecret>> {
        // A transient keyring failure (D-Bus hiccup, locked collection) must not
        // masquerade as "no secret" — that would flip the UI to unconfigured
        // while the secret still sits in the keyring. Only a clean NoEntry plus a
        // missing fallback file means Ok(None); other keyring errors surface as
        // Err so the poller can retry instead of parking on no-credential.
        let mut keyring_err: Option<anyhow::Error> = None;
        match self.keyring_entry() {
            Ok(entry) => match entry.get_password() {
                Ok(secret) => {
                    return Ok(Some(StoredSecret {
                        secret,
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
        if let Some(stored) = read_fallback_file(&self.fallback_path())? {
            return Ok(Some(stored));
        }
        // Windows: migrate a secret left in the pre-hardening roaming location to
        // the non-roaming dir, then drop the roaming plaintext copy.
        #[cfg(windows)]
        if let Some(legacy) = self.legacy_fallback_path()
            && let Some(stored) = read_fallback_file(&legacy)?
        {
            let _ = write_fallback_file(&self.fallback_path(), &stored.secret);
            let _ = std::fs::remove_file(&legacy);
            return Ok(Some(stored));
        }
        match keyring_err {
            Some(e) => Err(e),
            None => Ok(None),
        }
    }

    /// Remove the secret from both stores. Idempotent.
    pub fn clear(&self) -> Result<()> {
        if let Ok(entry) = self.keyring_entry() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => tracing::warn!("keyring delete failed: {e}"),
            }
        }
        self.remove_fallback_file()
    }

    fn remove_fallback_file(&self) -> Result<()> {
        remove_if_present(&self.fallback_path())?;
        // Windows: also drop any pre-hardening roaming copy so a cleared/rotated
        // secret can't survive in the old location.
        #[cfg(windows)]
        if let Some(legacy) = self.legacy_fallback_path() {
            remove_if_present(&legacy)?;
        }
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow!("removing {}: {e}", path.display())),
    }
}

fn write_fallback_file(path: &Path, secret: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("secret file path has no parent"))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating config dir {}", parent.display()))?;

    // Unique temp name per target file + process; create_new guarantees we
    // never open a pre-existing (possibly wider-mode) file.
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("credential.toml");
    let tmp = parent.join(format!(".{fname}.tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    // 0o600 on Unix; Windows has no POSIX mode bits — the per-user (non-roaming
    // local) dir ACL + Credential Manager (keyring) is the real boundary here.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp)
        .with_context(|| format!("creating secret temp file in {}", parent.display()))?;

    let body = toml::to_string(&FallbackFile {
        secret: secret.to_string(),
    })
    .map_err(|_| anyhow!("serializing secret file"))?;

    let write_result = file
        .write_all(body.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("writing secret temp file: {e}"));
    }
    drop(file);

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!("installing secret file at {}: {e}", path.display()));
    }
    Ok(())
}

fn read_fallback_file(path: &Path) -> Result<Option<StoredSecret>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("reading {}: {e}", path.display())),
    };
    // toml errors quote source snippets — never propagate them verbatim or the
    // secret leaks into the error string.
    let parsed: FallbackFile = toml::from_str(&raw)
        .map_err(|_| anyhow!("malformed secret file {} (redacted)", path.display()))?;
    Ok(Some(StoredSecret {
        secret: parsed.secret,
        on_disk: true,
    }))
}

// ── Tests (consolidated from clickup::auth + linear::auth) ──

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    const SECRET: &str = "pk_812345_SECRETSECRETSECRET";

    #[test]
    fn fallback_file_created_at_0600_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential.toml");

        write_fallback_file(&path, SECRET).unwrap();

        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let loaded = read_fallback_file(&path).unwrap().unwrap();
        assert_eq!(loaded.secret, SECRET);
        assert!(loaded.on_disk);
    }

    #[test]
    fn fallback_overwrite_keeps_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential.toml");
        write_fallback_file(&path, "first-secret").unwrap();
        write_fallback_file(&path, SECRET).unwrap();

        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert_eq!(read_fallback_file(&path).unwrap().unwrap().secret, SECRET);
        // No temp residue.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn fallback_write_error_never_contains_secret() {
        let dir = tempfile::tempdir().unwrap();
        // Parent is a file, so create_dir_all fails.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("sub").join("credential.toml");

        let err = write_fallback_file(&path, SECRET).unwrap_err();
        assert!(!format!("{err:#}").contains(SECRET));
    }

    #[test]
    fn malformed_fallback_error_never_contains_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential.toml");
        // Invalid TOML embedding the secret: the parse error must redact it.
        std::fs::write(&path, format!("secret = {SECRET}")).unwrap();

        let err = read_fallback_file(&path).unwrap_err();
        assert!(!format!("{err:#}").contains(SECRET));
    }

    #[test]
    fn missing_fallback_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            read_fallback_file(&dir.path().join("credential.toml"))
                .unwrap()
                .is_none()
        );
    }

    // Task 4.1b: both trackers' pre-existing on-disk TOML files must still
    // deserialize through the shared reader after the field-name unification —
    // a break here is a silent forced re-auth on the file-fallback path.
    #[test]
    fn legacy_clickup_token_field_deserializes_via_alias() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clickup.toml");
        std::fs::write(&path, format!("token = \"{SECRET}\"")).unwrap();

        let loaded = read_fallback_file(&path).unwrap().unwrap();
        assert_eq!(loaded.secret, SECRET);
    }

    #[test]
    fn legacy_linear_key_field_deserializes_via_alias() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.toml");
        std::fs::write(&path, format!("key = \"{SECRET}\"")).unwrap();

        let loaded = read_fallback_file(&path).unwrap().unwrap();
        assert_eq!(loaded.secret, SECRET);
    }

    // Task 4.3b: the constructed (service, account, filename) must equal the
    // pre-refactor literals byte-for-byte — a "tidied" account name makes an
    // existing user's keyring-stored secret invisible.
    #[test]
    fn clickup_constants_are_byte_identical_to_pre_refactor_literals() {
        let store = CredentialStore::new("nergal", "clickup-token", "clickup.toml");
        assert_eq!(store.service, "nergal");
        assert_eq!(store.account, "clickup-token");
        assert_eq!(store.filename, "clickup.toml");
    }

    #[test]
    fn linear_constants_are_byte_identical_to_pre_refactor_literals() {
        let store = CredentialStore::new("nergal", "linear-token", "linear.toml");
        assert_eq!(store.service, "nergal");
        assert_eq!(store.account, "linear-token");
        assert_eq!(store.filename, "linear.toml");
    }
}
