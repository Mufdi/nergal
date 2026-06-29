use std::path::{Path, PathBuf};

use crate::obsidian::config::ResolvedObsidianConfig;

pub fn vault_name_for(cfg: &ResolvedObsidianConfig) -> Option<String> {
    if let Some(name) = cfg.vault_name.as_deref().filter(|s| !s.is_empty()) {
        return Some(name.to_string());
    }
    cfg.vault_root
        .as_deref()
        .map(PathBuf::from)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
}

pub fn relative_to_vault(cfg: &ResolvedObsidianConfig, abs: &Path) -> Option<String> {
    let root = cfg.vault_root.as_deref()?;
    let root_pb = PathBuf::from(root);
    // canonicalize falls through to the raw path for not-yet-created files
    // so callers can resolve URIs for notes Obsidian is about to create.
    let root_canon = dunce::canonicalize(&root_pb).unwrap_or(root_pb);
    let abs_canon = dunce::canonicalize(abs).unwrap_or_else(|_| abs.to_path_buf());
    let rel = abs_canon.strip_prefix(&root_canon).ok()?;
    // Obsidian's `file=` parameter is '/'-separated on every OS; join the
    // components with '/' so a Windows '\' never leaks into the deep link
    // (which would percent-encode to a malformed `%5C` Obsidian can't resolve).
    let mut s = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    if let Some(stripped) = s.strip_suffix(".md") {
        s = stripped.to_string();
    }
    Some(s)
}

pub fn to_obsidian_uri(
    cfg: &ResolvedObsidianConfig,
    abs: &Path,
    heading: Option<&str>,
    block: Option<&str>,
) -> Option<String> {
    let vault = vault_name_for(cfg)?;
    let rel = relative_to_vault(cfg, abs)?;
    let mut uri = format!(
        "obsidian://open?vault={}&file={}",
        url_encode(&vault),
        url_encode(&rel),
    );
    // Obsidian accepts `#^block-id` with the caret literal but URL-encodes
    // the id; `#heading` URL-encodes the whole heading. Keep the shapes
    // distinct so existing Obsidian docs/links keep round-tripping.
    if let Some(h) = heading.filter(|s| !s.is_empty()) {
        uri.push('#');
        uri.push_str(&url_encode(h));
    } else if let Some(b) = block.filter(|s| !s.is_empty()) {
        uri.push_str("#^");
        uri.push_str(&url_encode(b));
    }
    Some(uri)
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let safe = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::config::ObsidianConfig;

    fn cfg_with(root: &str, name: Option<&str>) -> ResolvedObsidianConfig {
        ObsidianConfig {
            vault_root: Some(root.into()),
            vault_name: name.map(String::from),
            ..ObsidianConfig::defaults()
        }
    }

    #[test]
    fn vault_name_explicit_wins() {
        let c = cfg_with("/home/user/MyVault", Some("WorkVault"));
        assert_eq!(vault_name_for(&c).as_deref(), Some("WorkVault"));
    }

    #[test]
    fn vault_name_derived_from_basename() {
        let c = cfg_with("/home/user/MyVault", None);
        assert_eq!(vault_name_for(&c).as_deref(), Some("MyVault"));
    }

    #[test]
    fn relative_strips_md_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        std::fs::create_dir_all(vault.join("Projects")).unwrap();
        let note = vault.join("Projects").join("foo.md");
        std::fs::write(&note, b"").unwrap();
        let c = cfg_with(vault.to_str().unwrap(), None);
        assert_eq!(
            relative_to_vault(&c, &note).as_deref(),
            Some("Projects/foo")
        );
    }

    #[test]
    fn relative_returns_none_for_outside_path() {
        let c = cfg_with("/home/user/Vault", None);
        assert_eq!(relative_to_vault(&c, Path::new("/tmp/elsewhere.md")), None);
    }

    #[test]
    fn uri_url_encodes_spaces_and_slashes() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        std::fs::create_dir_all(vault.join("My Notes")).unwrap();
        let note = vault.join("My Notes").join("Project plan.md");
        std::fs::write(&note, b"").unwrap();
        let c = cfg_with(vault.to_str().unwrap(), Some("Vault"));
        let uri = to_obsidian_uri(&c, &note, None, None).unwrap();
        assert!(uri.contains("vault=Vault"));
        assert!(uri.contains("file=My%20Notes%2FProject%20plan"));
    }

    #[test]
    fn uri_with_heading() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let note = vault.join("Note.md");
        std::fs::write(&note, b"").unwrap();
        let c = cfg_with(vault.to_str().unwrap(), Some("V"));
        let uri = to_obsidian_uri(&c, &note, Some("Architecture"), None).unwrap();
        assert!(uri.ends_with("#Architecture"));
    }

    #[test]
    fn uri_with_block_ref() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let note = vault.join("Note.md");
        std::fs::write(&note, b"").unwrap();
        let c = cfg_with(vault.to_str().unwrap(), Some("V"));
        let uri = to_obsidian_uri(&c, &note, None, Some("abc123")).unwrap();
        assert!(uri.ends_with("#^abc123"));
    }

    #[test]
    fn uri_none_when_outside_vault() {
        let c = cfg_with("/home/user/Vault", Some("V"));
        assert_eq!(
            to_obsidian_uri(&c, Path::new("/tmp/foo.md"), None, None),
            None
        );
    }
}
