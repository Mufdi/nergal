//! Atomic file write: serialize to a sibling temp file, then `rename(2)` over
//! the destination. Rename is atomic on POSIX, so a crash, OOM-kill, or power
//! loss mid-write can never leave a truncated or partial file — a reader sees
//! either the complete old content or the complete new content, never a hybrid.
//!
//! Used for the config the app persists: its own `config.json`, the Claude Code
//! `settings.json`/`~/.claude.json`, the Codex/OpenCode MCP configs, the IPC
//! state file, and the rename-migration rewrites. A half-written `config.json`
//! silently resets the user's settings (the loader falls back to `default()`); a
//! half-written `~/.claude.json` corrupts a DIFFERENT app's state blob. Plain
//! `std::fs::write` truncates-then-writes and is exposed to exactly that window.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

/// Process-local counter so two concurrent writers (even to the same path) never
/// collide on the temp filename.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Write `contents` to `path` atomically (temp-sibling + rename). Creates the
/// parent directory if absent. The temp lives in the destination's own directory
/// so the rename stays on one filesystem (a cross-device rename would fail).
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("refusing to atomically write a path with no parent directory")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp");
    let tmp = parent.join(format!(".{stem}.{}.{seq}.tmp", std::process::id()));

    if let Err(e) = std::fs::write(&tmp, contents.as_ref())
        .with_context(|| format!("writing temp file {}", tmp.display()))
    {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    // Preserve the destination's permission bits if it already exists, so an
    // atomic overwrite never silently relaxes a tightened config to the umask
    // default. Mirrors how the conditional-wrapper migration carries the +x bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(
                &tmp,
                std::fs::Permissions::from_mode(meta.permissions().mode()),
            );
        }
    }

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow::Error::from(e).context(format!(
            "renaming {} -> {}",
            tmp.display(),
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_atomic(&path, b"{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn overwrites_existing_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "old").unwrap();
        write_atomic(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        // No leaked temp siblings after a successful write.
        let leftover: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftover.is_empty(), "temp file leaked: {leftover:?}");
    }

    #[test]
    fn creates_missing_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("state.json");
        write_atomic(&path, "x").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_existing_permission_bits() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.json");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        write_atomic(&path, "new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "atomic overwrite must keep the 0600 bits");
    }
}
