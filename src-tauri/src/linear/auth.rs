//! Linear Personal API key storage + the OAuth-extensible auth header.
//!
//! The keyring/fallback-file mechanics live in `tracker_shared::credential_store`
//! (design D3). This module wraps `CredentialStore` with what's genuinely
//! Linear-specific: `AuthMode`/`authorization_header_value` (the OAuth-extensible
//! header seam) and multi-workspace namespacing (`validate_org_id`,
//! `account_for`, `fallback_path_for`) — none of which has a ClickUp
//! equivalent, so they stay outside the shared struct rather than becoming a
//! dead field on it.

use anyhow::{Result, bail};

use crate::tracker_shared::credential_store::{CredentialStore, StoredSecret};

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

/// In-memory view of a stored key. `key` names the field (not `secret`) to
/// keep this module's public surface unchanged for its callers.
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

impl From<StoredSecret> for StoredKey {
    fn from(s: StoredSecret) -> Self {
        StoredKey {
            key: s.secret,
            on_disk: s.on_disk,
        }
    }
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

/// Per-workspace 0600 fallback filename when the keyring is unavailable.
/// `org_id` is a Linear UUID (no path separators); safe as a filename component.
fn fallback_path_for(org_id: &str) -> String {
    format!("linear-{org_id}.toml")
}

fn store_for(org_id: &str) -> CredentialStore {
    CredentialStore::new("nergal", account_for(org_id), fallback_path_for(org_id))
}

fn legacy_store() -> CredentialStore {
    CredentialStore::new("nergal", KEYRING_ACCOUNT, FALLBACK_FILE)
}

/// Store the active-workspace key (per-org account). Returns `true` if on-disk.
pub fn store_key_for(org_id: &str, key: &str) -> Result<bool> {
    validate_org_id(org_id)?;
    store_for(org_id).store(key)
}

/// Load a workspace's key (per-org account). `None` when neither store has one.
pub fn load_key_for(org_id: &str) -> Result<Option<StoredKey>> {
    validate_org_id(org_id)?;
    Ok(store_for(org_id).load()?.map(Into::into))
}

/// Remove a workspace's key from both stores. Idempotent.
pub fn remove_key_for(org_id: &str) -> Result<()> {
    validate_org_id(org_id)?;
    store_for(org_id).clear()
}

/// Store the legacy single key; returns `true` when it landed on disk. Kept so
/// the legacy `linear_set_key` path still works (the next poll migrates it to a
/// per-workspace entry).
pub fn store_key(key: &str) -> Result<bool> {
    legacy_store().store(key)
}

/// Load the legacy single key (the pre-multi-workspace store). A transient
/// keyring failure surfaces as Err (not Ok(None)) so the migration retries
/// instead of treating it as "no legacy key". Used only by the one-time
/// migration.
pub fn load_key() -> Result<Option<StoredKey>> {
    Ok(legacy_store().load()?.map(Into::into))
}

/// Remove the legacy key from both stores. Idempotent. Called after the legacy
/// key has been migrated into a per-workspace entry.
pub fn clear_key() -> Result<()> {
    legacy_store().clear()
}

#[cfg(test)]
mod tests {
    use super::*;

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
