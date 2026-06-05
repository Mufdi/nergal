//! Assembles pinned vault-note bodies into a single context block that seeds
//! an agent session at spawn/resume (obsidian-context-injection #3/#H).

use std::path::Path;

/// Total budget for the assembled context (≈ half the 128KB session-log cap),
/// keeping the injected system prompt sane on large pin sets.
const CONTEXT_BUDGET_BYTES: usize = 64 * 1024;

/// Whether `path` canonicalizes to a location under `vault_root`. The guard for
/// every read of a stored pin path: a pin command can't be trusted to have
/// vetted the path, and a DB row may predate any guard.
pub fn is_within_vault(vault_root: &Path, path: &str) -> bool {
    match (
        std::fs::canonicalize(vault_root),
        std::fs::canonicalize(path),
    ) {
        (Ok(root), Ok(p)) => p.starts_with(&root),
        _ => false,
    }
}

/// Read each pinned note (skipping unreadable ones) and wrap its body under a
/// labeled `## [[name]]` heading inside a `# Pinned vault context` block.
///
/// Returns `None` when there is nothing to inject (empty input, or every path
/// unreadable) so callers can leave the spawn command untouched. Caps the
/// total at [`CONTEXT_BUDGET_BYTES`]; on overflow it includes notes until the
/// budget is hit and appends a truncation marker (no silent cap — RULES).
pub fn assemble_context(paths: &[String], vault_root: Option<&str>) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    // No vault to validate against → nothing safe to inject. Pins only exist
    // when a vault is configured, so this is a defensive guard, not a flow.
    let Some(root) = vault_root.map(Path::new) else {
        tracing::warn!("pinned context requested without a configured vault_root; skipping");
        return None;
    };

    let mut out = String::from("# Pinned vault context\n");
    let mut included = 0usize;
    let mut dropped = 0usize;

    for path in paths {
        if !is_within_vault(root, path) {
            tracing::warn!(path = %path, "pinned note outside the vault; skipping (path guard)");
            continue;
        }
        let body = match std::fs::read_to_string(path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "pinned note unreadable; skipping");
                continue;
            }
        };
        let title = wikilink_name(path, vault_root);
        let block = format!("\n## [[{title}]]\n\n{}\n", body.trim_end());

        if out.len() + block.len() <= CONTEXT_BUDGET_BYTES {
            out.push_str(&block);
            included += 1;
        } else if included == 0 {
            // A first note larger than the whole budget would otherwise yield an
            // empty injection; include a char-boundary-safe slice instead.
            let room = CONTEXT_BUDGET_BYTES.saturating_sub(out.len());
            let mut cut = room.min(block.len());
            while cut > 0 && !block.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push_str(&block[..cut]);
            included += 1;
            dropped += 1;
        } else {
            dropped += 1;
        }
    }

    if included == 0 {
        return None;
    }
    if dropped > 0 {
        tracing::warn!(dropped, "pinned context truncated to fit budget");
        out.push_str(&format!(
            "\n\n_[Pinned context truncated: {dropped} note(s) omitted to fit the {}KB budget.]_\n",
            CONTEXT_BUDGET_BYTES / 1024
        ));
    }
    Some(out)
}

/// Obsidian wikilink target for a pinned note: the vault-relative path without
/// the `.md` extension when the note is under `vault_root` (disambiguates notes
/// that share a filename across folders), else the bare file stem.
fn wikilink_name(path: &str, vault_root: Option<&str>) -> String {
    let p = Path::new(path);
    let rel = vault_root
        .map(Path::new)
        .and_then(|root| p.strip_prefix(root).ok())
        .unwrap_or(p);
    rel.with_extension("")
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_note(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path.display().to_string()
    }

    #[test]
    fn empty_list_is_none() {
        assert!(assemble_context(&[], None).is_none());
    }

    #[test]
    fn all_missing_is_none() {
        let paths = vec!["/nope/missing.md".to_string()];
        assert!(assemble_context(&paths, None).is_none());
    }

    #[test]
    fn single_note_wraps_with_header_and_wikilink() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_note(dir.path(), "Alpha.md", "hello body");
        let out = assemble_context(&[p], Some(dir.path().to_str().unwrap())).unwrap();
        assert!(out.starts_with("# Pinned vault context\n"));
        assert!(out.contains("## [[Alpha]]"));
        assert!(out.contains("hello body"));
    }

    #[test]
    fn vault_relative_name_keeps_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_note(dir.path(), "Projects/Beta.md", "b");
        let out = assemble_context(&[p], Some(dir.path().to_str().unwrap())).unwrap();
        assert!(out.contains("## [[Projects/Beta]]"));
    }

    #[test]
    fn multi_note_order_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_note(dir.path(), "A.md", "aaa");
        let b = write_note(dir.path(), "B.md", "bbb");
        let out = assemble_context(&[a, b], Some(dir.path().to_str().unwrap())).unwrap();
        let ia = out.find("[[A]]").unwrap();
        let ib = out.find("[[B]]").unwrap();
        assert!(ia < ib);
    }

    #[test]
    fn path_outside_vault_is_skipped() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inside = write_note(vault.path(), "In.md", "inside body");
        let evil = write_note(outside.path(), "Secret.md", "leak me");
        let out = assemble_context(&[evil, inside], Some(vault.path().to_str().unwrap())).unwrap();
        assert!(out.contains("inside body"));
        assert!(!out.contains("leak me"));
    }

    #[test]
    fn no_vault_root_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_note(dir.path(), "A.md", "x");
        assert!(assemble_context(&[p], None).is_none());
    }

    #[test]
    fn oversize_emits_truncation_marker() {
        let dir = tempfile::tempdir().unwrap();
        let big = "x".repeat(70 * 1024);
        let a = write_note(dir.path(), "Big.md", &big);
        let b = write_note(dir.path(), "Dropped.md", "small");
        let out = assemble_context(&[a, b], Some(dir.path().to_str().unwrap())).unwrap();
        assert!(out.contains("## [[Big]]"));
        assert!(!out.contains("## [[Dropped]]"));
        assert!(out.contains("truncated"));
        assert!(out.len() <= CONTEXT_BUDGET_BYTES + 200);
    }
}
