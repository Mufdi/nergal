//! Scratchpad: cross-project, cross-session quick-notes buffer backed by `.md`
//! files on disk. Identity is a UUID v4 embedded in the filename
//! (`scratch-{uuid}.md`) so metadata in SQLite is stable across path/rename.
//!
//! This module owns the filesystem layer: read/write/list/create/soft-delete,
//! atomic writes via `tmp + rename` in the same directory, soft-delete with
//! epoch-in-filename for a deterministic 30-day purge, path traversal +
//! symlink rejection on every op, and a per-file ring buffer of last 8
//! self-write hashes for own-write tracking against the watcher.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub mod commands;
pub mod purge;
pub mod watcher;

/// 30 days in milliseconds. Soft-deleted files older than this on startup
/// are purged.
pub const TRASH_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Hard cap on file size we read into memory for hashing or display. Files
/// larger than this in `scratchpadPath` are ignored by the watcher to avoid
/// blocking the loop.
pub const MAX_NOTE_BYTES: u64 = 1024 * 1024;

/// Ring buffer depth for own-write hash tracking. Eight is enough to absorb
/// rapid autosaves with reorder/coalesce on FUSE filesystems.
const OWN_WRITE_RING_DEPTH: usize = 8;

/// Canonical filename pattern: `scratch-{uuid v4}.md`. Lowercase hex with
/// hyphens. Version-nibble is NOT enforced to keep the filter laxer than
/// strict v4 — any UUID-shaped hex group passes, which is what we generate.
fn note_filename_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^scratch-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$")
            .expect("static regex")
    })
}

/// Trash filename pattern: `scratch-{uuid}-trashed-{epoch_ms}.md`. We capture
/// the epoch group for purge.
pub fn trash_filename_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^scratch-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}-trashed-(\d+)\.md$",
        )
        .expect("static regex")
    })
}

/// Extract the UUID part from a `scratch-{uuid}.md` filename.
pub fn tab_id_from_filename(name: &str) -> Option<String> {
    if !note_filename_re().is_match(name) {
        return None;
    }
    let stem = name.strip_suffix(".md")?;
    let uuid_part = stem.strip_prefix("scratch-")?;
    Some(uuid_part.to_string())
}

/// Build the canonical filename for a given tab_id (UUID).
fn note_filename_for(tab_id: &str) -> String {
    format!("scratch-{tab_id}.md")
}

/// Tab payload returned to the frontend. The display name "Scratch N" is
/// derived in the frontend from `position`; the backend never numbers them.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScratchTab {
    pub tab_id: String,
    pub position: i64,
    pub last_modified_ms: i64,
}

/// Per-file ring buffer of last N self-write SHA-256 hashes. Used by the
/// watcher to filter out own-writes regardless of notify timing.
#[derive(Default)]
pub struct OwnWriteHashes {
    inner: Mutex<HashMap<String, VecDeque<[u8; 32]>>>,
}

impl OwnWriteHashes {
    pub fn record(&self, tab_id: &str, hash: [u8; 32]) {
        let mut guard = self.inner.lock().expect("own-write mutex");
        let ring = guard.entry(tab_id.to_string()).or_default();
        if ring.len() >= OWN_WRITE_RING_DEPTH {
            ring.pop_front();
        }
        ring.push_back(hash);
    }

    pub fn contains(&self, tab_id: &str, hash: &[u8; 32]) -> bool {
        let guard = self.inner.lock().expect("own-write mutex");
        match guard.get(tab_id) {
            Some(ring) => ring.iter().any(|h| h == hash),
            None => false,
        }
    }

    pub fn forget(&self, tab_id: &str) {
        let mut guard = self.inner.lock().expect("own-write mutex");
        guard.remove(tab_id);
    }

    /// Drop all tracked hashes. Used on path change.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().expect("own-write mutex");
        guard.clear();
    }
}

/// Compute SHA-256 of a byte slice.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Default scratchpad directory: `~/.config/cluihud/scratchpad/`.
pub fn default_scratchpad_dir() -> PathBuf {
    let base =
        dirs::config_dir().unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
    base.join("cluihud").join("scratchpad")
}

/// Ensure the scratchpad directory exists. Creates parent dirs if needed.
/// Returns the canonical path on success.
pub fn ensure_dir(scratchpad_path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(scratchpad_path)
        .with_context(|| format!("creating scratchpad dir: {}", scratchpad_path.display()))?;
    let trash_dir = scratchpad_path.join(".trash");
    fs::create_dir_all(&trash_dir)
        .with_context(|| format!("creating trash dir: {}", trash_dir.display()))?;
    let canonical = scratchpad_path
        .canonicalize()
        .with_context(|| format!("canonicalizing: {}", scratchpad_path.display()))?;
    reject_symlink_dir(&canonical)?;
    Ok(canonical)
}

/// Reject a path whose canonical form is itself a symlink (paranoid check —
/// `canonicalize` resolves symlinks in components but the leaf can still be
/// a symlink to a dir).
fn reject_symlink_dir(canonical: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(canonical)
        .with_context(|| format!("symlink_metadata: {}", canonical.display()))?;
    if meta.is_symlink() {
        bail!("refusing to operate on symlink: {}", canonical.display());
    }
    Ok(())
}

/// Validate that `target` lives inside `scratchpad_root` and is not a symlink.
/// Both paths are canonicalized; the resolved target must start_with the
/// resolved root. This blocks path traversal, symlink-to-outside, and
/// symlink-as-file disclosure.
fn validate_target(scratchpad_root: &Path, target: &Path) -> Result<PathBuf> {
    let root = scratchpad_root
        .canonicalize()
        .with_context(|| format!("canonicalize root: {}", scratchpad_root.display()))?;
    let canonical_target = if target.exists() {
        target
            .canonicalize()
            .with_context(|| format!("canonicalize target: {}", target.display()))?
    } else {
        let parent_path = target
            .parent()
            .ok_or_else(|| anyhow!("target has no parent: {}", target.display()))?;
        let canonical_parent = parent_path
            .canonicalize()
            .with_context(|| format!("canonicalize parent: {}", parent_path.display()))?;
        let name = target
            .file_name()
            .ok_or_else(|| anyhow!("target has no file_name: {}", target.display()))?;
        canonical_parent.join(name)
    };
    if !canonical_target.starts_with(&root) {
        bail!(
            "path escapes scratchpad root: target={} root={}",
            canonical_target.display(),
            root.display()
        );
    }
    if target.exists() {
        let meta = fs::symlink_metadata(target)
            .with_context(|| format!("symlink_metadata: {}", target.display()))?;
        if meta.is_symlink() {
            bail!("refusing symlink: {}", target.display());
        }
    }
    Ok(canonical_target)
}

/// List scratchpad notes in `scratchpad_root`. Skips `.trash/`, dotfiles,
/// non-matching filenames, symlinks, and files larger than `MAX_NOTE_BYTES`.
pub fn list_notes(scratchpad_root: &Path) -> Result<Vec<ScratchTab>> {
    let root = ensure_dir(scratchpad_root)?;
    let pattern = note_filename_re();
    let mut entries: Vec<(String, i64)> = Vec::new();

    for entry in fs::read_dir(&root).with_context(|| format!("read_dir: {}", root.display()))? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if !pattern.is_match(name) {
            continue;
        }
        let path = entry.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_symlink() || !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_NOTE_BYTES {
            continue;
        }
        let Some(tab_id) = tab_id_from_filename(name) else {
            continue;
        };
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        entries.push((tab_id, mtime_ms));
    }

    entries.sort_by_key(|(_, mtime)| *mtime);

    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(i, (tab_id, mtime_ms))| ScratchTab {
            tab_id,
            position: i as i64,
            last_modified_ms: mtime_ms,
        })
        .collect())
}

/// Read a scratchpad note's contents as UTF-8 text. Files exceeding
/// `MAX_NOTE_BYTES` are rejected.
pub fn read_note(scratchpad_root: &Path, tab_id: &str) -> Result<String> {
    let root = ensure_dir(scratchpad_root)?;
    let target = root.join(note_filename_for(tab_id));
    validate_target(&root, &target)?;
    let meta = fs::metadata(&target).with_context(|| format!("metadata: {}", target.display()))?;
    if meta.len() > MAX_NOTE_BYTES {
        bail!("note exceeds size cap: {}", target.display());
    }
    fs::read_to_string(&target).with_context(|| format!("read_to_string: {}", target.display()))
}

/// Atomically write `content` to the note at `tab_id`. Tmp file lives in the
/// same directory as the target so `rename(2)` is atomic regardless of the
/// filesystem `scratchpadPath` lives on. Returns the SHA-256 of the bytes
/// written so the caller can stamp the own-write ring buffer.
pub fn write_note(scratchpad_root: &Path, tab_id: &str, content: &str) -> Result<[u8; 32]> {
    let root = ensure_dir(scratchpad_root)?;
    let target = root.join(note_filename_for(tab_id));
    validate_target(&root, &target)?;
    let tmp = root.join(format!(".scratch-{tab_id}.md.tmp"));
    fs::write(&tmp, content.as_bytes()).with_context(|| format!("write tmp: {}", tmp.display()))?;
    fs::rename(&tmp, &target)
        .with_context(|| format!("rename {} → {}", tmp.display(), target.display()))?;
    Ok(sha256(content.as_bytes()))
}

/// Create a new empty note. Returns the freshly minted tab_id (UUID v4).
pub fn create_note(scratchpad_root: &Path) -> Result<String> {
    let root = ensure_dir(scratchpad_root)?;
    let tab_id = Uuid::new_v4().to_string();
    let target = root.join(note_filename_for(&tab_id));
    validate_target(&root, &target)?;
    fs::write(&target, "").with_context(|| format!("create empty: {}", target.display()))?;
    Ok(tab_id)
}

/// Soft-delete: rename to `.trash/scratch-{uuid}-trashed-{epoch_ms}.md`.
/// Filename embeds the trash time so purge does not depend on `mtime`.
pub fn soft_delete(scratchpad_root: &Path, tab_id: &str) -> Result<()> {
    let root = ensure_dir(scratchpad_root)?;
    let source = root.join(note_filename_for(tab_id));
    validate_target(&root, &source)?;
    if !source.exists() {
        return Ok(());
    }
    let trash_dir = root.join(".trash");
    fs::create_dir_all(&trash_dir)
        .with_context(|| format!("ensure trash: {}", trash_dir.display()))?;
    let trashed_name = format!("scratch-{tab_id}-trashed-{}.md", now_ms());
    let dest = trash_dir.join(trashed_name);
    fs::rename(&source, &dest)
        .with_context(|| format!("trash rename {} → {}", source.display(), dest.display()))?;
    Ok(())
}

/// Restore the most recently trashed copy of `tab_id` back into the
/// scratchpad root. Picks the file with the largest embedded epoch
/// (i.e. last to be trashed) when multiple exist. No-op if nothing matches.
pub fn restore_from_trash(scratchpad_root: &Path, tab_id: &str) -> Result<bool> {
    let root = ensure_dir(scratchpad_root)?;
    let trash_dir = root.join(".trash");
    if !trash_dir.exists() {
        return Ok(false);
    }
    let prefix = format!("scratch-{tab_id}-trashed-");
    let pattern = trash_filename_re();
    let mut best: Option<(i64, PathBuf)> = None;
    for entry in fs::read_dir(&trash_dir)? {
        let Ok(entry) = entry else { continue };
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let Some(caps) = pattern.captures(name) else {
            continue;
        };
        let Some(epoch_str) = caps.get(1) else {
            continue;
        };
        let epoch: i64 = match epoch_str.as_str().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        match &best {
            Some((current, _)) if *current >= epoch => {}
            _ => best = Some((epoch, entry.path())),
        }
    }
    let Some((_, source)) = best else {
        return Ok(false);
    };
    let target = root.join(note_filename_for(tab_id));
    if target.exists() {
        return Ok(false);
    }
    fs::rename(&source, &target)
        .with_context(|| format!("restore {} → {}", source.display(), target.display()))?;
    Ok(true)
}

/// Cleanup orphan `.tmp` files from previous crashes.
pub fn cleanup_orphan_tmps(scratchpad_root: &Path) -> Result<usize> {
    let root = ensure_dir(scratchpad_root)?;
    let mut removed = 0;
    for entry in fs::read_dir(&root)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with(".scratch-") && name_str.ends_with(".md.tmp") {
            let _ = fs::remove_file(entry.path());
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("cluihud-scratch-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn ensure_dir_creates_root_and_trash() {
        let dir = tmpdir();
        let canonical = ensure_dir(&dir).unwrap();
        assert!(canonical.exists());
        assert!(canonical.join(".trash").exists());
    }

    #[test]
    fn create_then_list_returns_one_tab() {
        let dir = tmpdir();
        let tab_id = create_note(&dir).unwrap();
        let tabs = list_notes(&dir).unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].tab_id, tab_id);
        assert_eq!(tabs[0].position, 0);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tmpdir();
        let tab_id = create_note(&dir).unwrap();
        let content = "hello scratchpad";
        let hash = write_note(&dir, &tab_id, content).unwrap();
        assert_eq!(hash, sha256(content.as_bytes()));
        let got = read_note(&dir, &tab_id).unwrap();
        assert_eq!(got, content);
    }

    #[test]
    fn write_uses_tmp_in_same_dir_then_rename() {
        let dir = tmpdir();
        let tab_id = create_note(&dir).unwrap();
        write_note(&dir, &tab_id, "x").unwrap();
        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        // Only the final file should exist; no .tmp left behind.
        assert!(
            entries
                .iter()
                .any(|n| n.starts_with("scratch-") && n.ends_with(".md"))
        );
        assert!(!entries.iter().any(|n| n.ends_with(".md.tmp")));
    }

    #[test]
    fn soft_delete_moves_to_trash_with_epoch() {
        let dir = tmpdir();
        let tab_id = create_note(&dir).unwrap();
        write_note(&dir, &tab_id, "bye").unwrap();
        soft_delete(&dir, &tab_id).unwrap();

        let listing = list_notes(&dir).unwrap();
        assert!(listing.is_empty());

        let trash_entries: Vec<_> = fs::read_dir(dir.join(".trash"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(trash_entries.len(), 1);
        let name = &trash_entries[0];
        assert!(trash_filename_re().is_match(name), "got: {name}");
    }

    #[test]
    fn list_filters_dotfiles_and_unmatched() {
        let dir = tmpdir();
        ensure_dir(&dir).unwrap();
        // Valid note.
        let tab_id = create_note(&dir).unwrap();
        // Garbage we should ignore.
        fs::write(dir.join("readme.md"), "not a scratch").unwrap();
        fs::write(dir.join(".scratch-fake.md.tmp"), "stale tmp").unwrap();
        fs::write(dir.join("scratch-not-a-uuid.md"), "wrong shape").unwrap();
        let tabs = list_notes(&dir).unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].tab_id, tab_id);
    }

    #[test]
    fn list_rejects_symlink_into_root() {
        let dir = tmpdir();
        ensure_dir(&dir).unwrap();
        // Create a symlink inside scratchpad pointing to /etc/hostname (or
        // any readable file outside). The list must NOT surface it.
        let target = dir.join("scratch-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.md");
        symlink("/etc/hostname", &target).unwrap_or(());
        let tabs = list_notes(&dir).unwrap();
        assert!(tabs.is_empty(), "symlink leaked into listing");
    }

    #[test]
    fn write_outside_root_is_rejected() {
        let dir = tmpdir();
        ensure_dir(&dir).unwrap();
        // tab_id with a slash should not be accepted by note_filename_for,
        // but we test the path-validation path explicitly via a crafted
        // call: write to a file whose canonical parent is outside.
        let outside = std::env::temp_dir().join("escape.md");
        let res = validate_target(&dir, &outside);
        assert!(res.is_err(), "expected path-escape rejection");
    }

    #[test]
    fn own_write_ring_absorbs_recent_hashes() {
        let ring = OwnWriteHashes::default();
        let h1 = sha256(b"a");
        let h2 = sha256(b"b");
        ring.record("t1", h1);
        ring.record("t1", h2);
        assert!(ring.contains("t1", &h1));
        assert!(ring.contains("t1", &h2));
        assert!(!ring.contains("t1", &sha256(b"c")));
    }

    #[test]
    fn own_write_ring_evicts_oldest_after_8() {
        let ring = OwnWriteHashes::default();
        let hashes: Vec<_> = (0..10).map(|i| sha256(&[i as u8])).collect();
        for h in &hashes {
            ring.record("t", *h);
        }
        // The first two should have been evicted.
        assert!(!ring.contains("t", &hashes[0]));
        assert!(!ring.contains("t", &hashes[1]));
        assert!(ring.contains("t", &hashes[2]));
        assert!(ring.contains("t", &hashes[9]));
    }

    #[test]
    fn cleanup_orphan_tmps_removes_stale() {
        let dir = tmpdir();
        ensure_dir(&dir).unwrap();
        fs::write(dir.join(".scratch-deadbeef.md.tmp"), "stale").unwrap();
        let removed = cleanup_orphan_tmps(&dir).unwrap();
        assert_eq!(removed, 1);
        assert!(!dir.join(".scratch-deadbeef.md.tmp").exists());
    }
}
