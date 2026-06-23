//! OS-keyring storage for the summary API key (phase 6, `api_key` mode).
//!
//! Unlike `clickup/auth.rs`, there is **no plaintext fallback**: the user's
//! explicit constraint is that this key never touch `config.json` or any
//! on-disk file. If the keyring is unavailable the key simply cannot be stored
//! and the API-key backend stays unconfigured — the Agent-CLI mode (key-free)
//! remains available.
//!
//! Leak guard: no function here may embed the key in an error string or log.

use anyhow::{Result, anyhow, bail};

const KEYRING_SERVICE: &str = "nergal";
const KEYRING_ACCOUNT: &str = "summary-api-key";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| anyhow!("keyring entry init: {e}"))
}

/// Store the API key in the OS keyring. Errors if the keyring is unavailable —
/// we never silently fall back to plaintext.
pub fn store_api_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        bail!("api key is empty");
    }
    entry()?
        .set_password(key)
        .map_err(|e| anyhow!("keyring write failed: {e}"))
}

/// Load the API key, or `None` when none is stored. A transient keyring error
/// surfaces as `Err` so the caller can distinguish "not configured" from "the
/// keyring is locked".
pub fn load_api_key() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!("keyring read failed: {e}")),
    }
}

/// True when an API key is present (without returning it).
pub fn has_api_key() -> Result<bool> {
    Ok(load_api_key()?.is_some())
}

/// Remove the API key from the keyring. Idempotent.
pub fn clear_api_key() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("keyring delete failed: {e}")),
    }
}
