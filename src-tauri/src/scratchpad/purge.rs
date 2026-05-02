//! Purge soft-deleted scratchpad notes older than `TRASH_TTL_MS` days.
//! Uses the epoch embedded in the trash filename, NOT `mtime` (which would
//! incorrectly trigger on freshly trashed files whose original mtime was old).

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use super::{TRASH_TTL_MS, trash_filename_re};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Walk `<scratchpad_root>/.trash/` and remove files whose filename-embedded
/// epoch is older than `TRASH_TTL_MS`. Returns the count purged.
pub fn purge_trash(scratchpad_root: &Path) -> Result<usize> {
    let trash_dir = scratchpad_root.join(".trash");
    if !trash_dir.exists() {
        return Ok(0);
    }
    let cutoff = now_ms() - TRASH_TTL_MS;
    let mut removed = 0;
    let pattern = trash_filename_re();
    for entry in fs::read_dir(&trash_dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let Some(caps) = pattern.captures(name_str) else {
            continue;
        };
        let Some(epoch_str) = caps.get(1) else {
            continue;
        };
        let epoch: i64 = match epoch_str.as_str().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if epoch < cutoff {
            // Symlink check — the trash dir can be tampered with.
            if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                if meta.is_symlink() {
                    continue;
                }
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    fn tmpdir() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("cluihud-purge-{}", Uuid::new_v4()));
        fs::create_dir_all(p.join(".trash")).unwrap();
        p
    }

    #[test]
    fn purge_removes_old_files_only() {
        let dir = tmpdir();
        let trash = dir.join(".trash");
        let uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

        let old_epoch = now_ms() - TRASH_TTL_MS - 1000;
        let old_name = format!("scratch-{uuid}-trashed-{old_epoch}.md");
        fs::write(trash.join(&old_name), "old").unwrap();

        let fresh_epoch = now_ms() - 1000;
        let uuid2 = "11111111-2222-3333-4444-555555555555";
        let fresh_name = format!("scratch-{uuid2}-trashed-{fresh_epoch}.md");
        fs::write(trash.join(&fresh_name), "fresh").unwrap();

        let n = purge_trash(&dir).unwrap();
        assert_eq!(n, 1);
        assert!(!trash.join(&old_name).exists());
        assert!(trash.join(&fresh_name).exists());
    }

    #[test]
    fn purge_skips_files_without_epoch_pattern() {
        let dir = tmpdir();
        let trash = dir.join(".trash");
        fs::write(trash.join("readme.md"), "garbage").unwrap();
        fs::write(trash.join(".dotfile"), "hidden").unwrap();
        let n = purge_trash(&dir).unwrap();
        assert_eq!(n, 0);
        assert!(trash.join("readme.md").exists());
    }
}
