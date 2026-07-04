//! ClickUp Personal API token storage.
//!
//! The keyring/fallback-file mechanics live in `tracker_shared::credential_store`
//! (design D3). ClickUp has no multi-workspace or OAuth seam, so this module is
//! a flat single-account wrapper around one `CredentialStore`.

use anyhow::Result;

use crate::tracker_shared::credential_store::{CredentialStore, StoredSecret};

const KEYRING_ACCOUNT: &str = "clickup-token";
const FALLBACK_FILE: &str = "clickup.toml";

fn store() -> CredentialStore {
    CredentialStore::new("nergal", KEYRING_ACCOUNT, FALLBACK_FILE)
}

/// In-memory view of a stored token. `token` names the field (not `secret`) to
/// keep this module's public surface unchanged for its callers.
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

impl From<StoredSecret> for StoredToken {
    fn from(s: StoredSecret) -> Self {
        StoredToken {
            token: s.secret,
            on_disk: s.on_disk,
        }
    }
}

/// Store the token; returns `true` when it landed in the on-disk fallback.
pub fn store_token(token: &str) -> Result<bool> {
    store().store(token)
}

/// Load the token: keyring first, fallback file second. `None` when neither
/// store has one.
pub fn load_token() -> Result<Option<StoredToken>> {
    Ok(store().load()?.map(Into::into))
}

/// Remove the token from both stores. Idempotent.
pub fn clear_token() -> Result<()> {
    store().clear()
}
