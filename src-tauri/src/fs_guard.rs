//! Path-containment guard for session-scoped filesystem commands
//! (harden-fs-command-path-traversal). The webview IPC boundary is not a
//! trust boundary, so every command that joins a caller-supplied path onto a
//! session base (cwd or openspec dir) must confine the result to that base.
//! Mirrors `obsidian::pinned_notes::is_within_vault`'s canonicalize +
//! `starts_with` discipline, generalized to return the vetted path so callers
//! never re-join the raw input themselves.

use std::path::{Component, Path, PathBuf};

/// Resolve `rel` against `base`, confined to `base`'s canonical form.
///
/// - Any `..` (`Component::ParentDir`) in `rel` is rejected up front.
/// - An absolute `rel` is treated as the target directly (not joined onto
///   `base`) and accepted **only if** it canonicalizes inside `base`. This is
///   deliberate, not a hole: editor tabs opened from Activities or the
///   `open-file` deep link store absolute `filePath`s, so blanket-rejecting
///   absolute input would break that flow. The confinement check is
///   identical either way — both branches end in the same `starts_with`.
/// - A relative `rel` resolves as `base.join(rel)`.
/// - The target may not exist yet (a new file, or a nested dir a caller is
///   about to `create_dir_all`): the guard walks up to the nearest existing
///   ancestor, canonicalizes it, then lexically re-appends the remaining
///   (already `..`-free) components.
/// - Canonicalization resolves symlinks, so a symlink inside `base` pointing
///   outside is still caught by the final `starts_with` check (fail-closed).
///
/// Returns the vetted `PathBuf`. Callers must use it directly for the
/// filesystem operation rather than re-joining `rel` — using anything else
/// reopens the TOCTOU gap this guard closes.
pub(crate) fn resolve_within_base(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("path escapes the session directory".into());
    }

    let target = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        base.join(rel_path)
    };

    let canonical_base = dunce::canonicalize(base)
        .map_err(|e| format!("failed to resolve session directory: {e}"))?;

    let canonical_target = canonicalize_with_missing_tail(&target)
        .map_err(|_| "path escapes the session directory".to_string())?;

    if !canonical_target.starts_with(&canonical_base) {
        return Err("path escapes the session directory".into());
    }

    Ok(canonical_target)
}

/// Canonicalize `target`, walking up to the nearest existing ancestor when
/// the leaf (or several trailing components) do not exist yet, then
/// lexically re-appends the missing tail onto the canonicalized ancestor.
/// The tail is guaranteed `..`-free by the caller's up-front check, so the
/// lexical append cannot escape the ancestor a second time.
fn canonicalize_with_missing_tail(target: &Path) -> std::io::Result<PathBuf> {
    let mut missing_tail = Vec::new();
    let mut cursor = target;

    loop {
        match dunce::canonicalize(cursor) {
            Ok(canonical_ancestor) => {
                let mut result = canonical_ancestor;
                for component in missing_tail.iter().rev() {
                    result.push(component);
                }
                return Ok(result);
            }
            Err(e) => {
                let (Some(parent), Some(file_name)) = (cursor.parent(), cursor.file_name()) else {
                    return Err(e);
                };
                missing_tail.push(file_name.to_os_string());
                cursor = parent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkdir(base: &Path, rel: &str) -> PathBuf {
        let dir = base.join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(base: &Path, rel: &str) -> PathBuf {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn absolute_outside_base_rejected() {
        let base = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = touch(outside.path(), "secret.txt");
        let result = resolve_within_base(base.path(), secret.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn absolute_inside_base_accepted() {
        let base = tempfile::tempdir().unwrap();
        let file = touch(base.path(), "src/main.rs");
        let result = resolve_within_base(base.path(), file.to_str().unwrap()).unwrap();
        assert_eq!(result, dunce::canonicalize(&file).unwrap());
    }

    #[test]
    fn parent_dir_component_rejected() {
        let base = tempfile::tempdir().unwrap();
        touch(base.path(), "secret.txt");
        let result = resolve_within_base(base.path(), "../secret.txt");
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn symlink_escape_rejected() {
        let base = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        touch(outside.path(), "id_rsa");
        std::os::unix::fs::symlink(outside.path(), base.path().join("link")).unwrap();
        let result = resolve_within_base(base.path(), "link/id_rsa");
        assert!(result.is_err());
    }

    #[test]
    fn legitimate_nested_path_accepted() {
        let base = tempfile::tempdir().unwrap();
        let dir = mkdir(base.path(), "src/components");
        let result = resolve_within_base(base.path(), "src/components").unwrap();
        assert_eq!(result, dunce::canonicalize(&dir).unwrap());
    }

    #[test]
    fn new_file_in_base_accepted() {
        let base = tempfile::tempdir().unwrap();
        mkdir(base.path(), "notes");
        let result = resolve_within_base(base.path(), "notes/new.md").unwrap();
        assert_eq!(
            result,
            dunce::canonicalize(base.path())
                .unwrap()
                .join("notes/new.md")
        );
    }

    #[test]
    fn new_nested_dir_under_base_accepted() {
        let base = tempfile::tempdir().unwrap();
        mkdir(base.path(), "openspec/changes/x");
        let result =
            resolve_within_base(base.path(), "openspec/changes/x/specs/cap/spec.md").unwrap();
        assert_eq!(
            result,
            dunce::canonicalize(base.path())
                .unwrap()
                .join("openspec/changes/x/specs/cap/spec.md")
        );
    }

    #[test]
    fn change_name_traversal_rejected() {
        let base = tempfile::tempdir().unwrap();
        mkdir(base.path(), "changes/real-change");
        let result = resolve_within_base(base.path(), "../../..");
        assert!(result.is_err());
    }
}
