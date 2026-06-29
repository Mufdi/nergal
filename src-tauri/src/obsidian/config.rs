use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ObsidianConfig {
    pub vault_root: Option<String>,
    pub vault_name: Option<String>,
    pub session_log_path: Option<String>,
    pub quick_capture_path: Option<String>,
    pub moc_path: Option<String>,
    pub templates_path: Option<String>,
    pub backlinks_enabled: bool,
    pub render_wikilinks: bool,
    /// Vault-relative folder scoping vault search + the `@@` picker; None/empty = whole vault.
    pub search_subdir: Option<String>,
}

impl ObsidianConfig {
    pub fn defaults() -> Self {
        Self {
            backlinks_enabled: false,
            render_wikilinks: true,
            ..Default::default()
        }
    }
}

pub type ResolvedObsidianConfig = ObsidianConfig;

pub fn global_toml_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("nergal").join("obsidian.toml"))
}

// Obsidian hides non-.md files by default, so an unextensioned path would
// produce a capture file the user can't see inside Obsidian.
pub fn normalize_md_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let p = std::path::Path::new(trimmed);
    let basename = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if basename.is_empty() {
        return trimmed.to_string();
    }
    if basename.contains('.') {
        return trimmed.to_string();
    }
    format!("{trimmed}.md")
}

// std::fs::canonicalize doesn't expand `~` — that's a shell convention. Users
// type `~/Documents/...` in Settings expecting it to work, so we expand it
// here at save time.
pub fn expand_home(path: &str) -> String {
    let trimmed = path.trim();
    if !trimmed.starts_with('~') {
        return trimmed.to_string();
    }
    let Some(home) = dirs::home_dir() else {
        return trimmed.to_string();
    };
    let home = home.to_string_lossy().into_owned();
    if trimmed == "~" {
        return home;
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    trimmed.to_string()
}

// Linux filesystems are case-sensitive but users type paths from memory
// ("~/documents/obsidian23"). Walk the path segment by segment: when a
// segment doesn't exist verbatim and exactly one sibling matches
// case-insensitively, take the on-disk spelling. Correction stops at the
// first segment without a unique match — the tail may name files that
// don't exist yet (e.g. a quick-capture target), so it passes through as
// typed.
pub fn resolve_case_insensitive(path: &str) -> String {
    use std::path::Component;
    let p = Path::new(path);
    if !p.is_absolute() {
        return path.to_string();
    }
    let mut resolved = PathBuf::new();
    let mut correcting = true;
    for comp in p.components() {
        match comp {
            Component::Normal(seg) if correcting => {
                if resolved.join(seg).exists() {
                    resolved.push(seg);
                    continue;
                }
                let target = seg.to_string_lossy().to_lowercase();
                let mut matches = match std::fs::read_dir(&resolved) {
                    Ok(rd) => rd
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name())
                        .filter(|n| n.to_string_lossy().to_lowercase() == target)
                        .collect::<Vec<_>>(),
                    Err(_) => Vec::new(),
                };
                if matches.len() == 1 {
                    resolved.push(matches.remove(0));
                } else {
                    resolved.push(seg);
                    correcting = false;
                }
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved.to_string_lossy().into_owned()
}

// Trailing slashes from user input lead to double-slash bugs when we do
// naive `format!("{root}/Projects/...")`. Normalize at save time.
fn strip_trailing_slashes(s: &str) -> String {
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() && s.starts_with('/') {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn expand_field(field: &mut Option<String>) {
    if let Some(v) = field.as_deref() {
        let expanded = strip_trailing_slashes(&expand_home(v));
        *field = if expanded.is_empty() {
            None
        } else {
            Some(resolve_case_insensitive(&expanded))
        };
    }
}

pub fn normalize_file_channels(cfg: &mut ObsidianConfig) {
    expand_field(&mut cfg.vault_root);
    expand_field(&mut cfg.quick_capture_path);
    expand_field(&mut cfg.session_log_path);
    expand_field(&mut cfg.moc_path);
    expand_field(&mut cfg.templates_path);
    if let Some(p) = cfg.quick_capture_path.as_deref() {
        let n = normalize_md_path(p);
        cfg.quick_capture_path = if n.is_empty() { None } else { Some(n) };
    }
    if let Some(p) = cfg.session_log_path.as_deref() {
        let n = normalize_md_path(p);
        cfg.session_log_path = if n.is_empty() { None } else { Some(n) };
    }
    // Vault-relative: no ~ expansion. Trim surrounding slashes/whitespace so the
    // later vault_root.join() stays inside the vault; empty collapses to None.
    if let Some(s) = cfg.search_subdir.as_deref() {
        let n = s.trim().trim_matches('/').to_string();
        cfg.search_subdir = if n.is_empty() { None } else { Some(n) };
    }
}

pub fn apply_toml_override(mut cfg: ObsidianConfig, toml_path: &Path) -> ObsidianConfig {
    let Ok(contents) = std::fs::read_to_string(toml_path) else {
        return cfg;
    };
    let parsed: TomlOverride = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("obsidian: ignoring malformed {}: {e}", toml_path.display());
            return cfg;
        }
    };
    if let Some(v) = parsed.vault_root {
        cfg.vault_root = Some(v);
    }
    if let Some(v) = parsed.vault_name {
        cfg.vault_name = Some(v);
    }
    if let Some(v) = parsed.session_log_path {
        cfg.session_log_path = Some(v);
    }
    if let Some(v) = parsed.quick_capture_path {
        cfg.quick_capture_path = Some(v);
    }
    if let Some(v) = parsed.moc_path {
        cfg.moc_path = Some(v);
    }
    if let Some(v) = parsed.templates_path {
        cfg.templates_path = Some(v);
    }
    if let Some(v) = parsed.backlinks_enabled {
        cfg.backlinks_enabled = v;
    }
    if let Some(v) = parsed.render_wikilinks {
        cfg.render_wikilinks = v;
    }
    if let Some(v) = parsed.search_subdir {
        cfg.search_subdir = Some(v);
    }
    cfg
}

pub fn resolve<F>(workspace_id: &str, db_read: F) -> Result<ResolvedObsidianConfig>
where
    F: FnOnce(&str) -> Result<Option<ObsidianConfig>>,
{
    let base = db_read(workspace_id)
        .with_context(|| format!("loading obsidian_config for {workspace_id}"))?
        .unwrap_or_else(ObsidianConfig::defaults);
    let Some(toml_path) = global_toml_path() else {
        return Ok(base);
    };
    Ok(apply_toml_override(base, &toml_path))
}

// Option-wrapped fields so apply_toml_override can distinguish "field absent"
// from "field explicitly null" — null on a SQLite-overridden field would
// clobber the DB value silently otherwise.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TomlOverride {
    vault_root: Option<String>,
    vault_name: Option<String>,
    session_log_path: Option<String>,
    quick_capture_path: Option<String>,
    moc_path: Option<String>,
    templates_path: Option<String>,
    backlinks_enabled: Option<bool>,
    render_wikilinks: Option<bool>,
    search_subdir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_schema() {
        let d = ObsidianConfig::defaults();
        assert!(!d.backlinks_enabled);
        assert!(d.render_wikilinks);
        assert!(d.vault_root.is_none());
    }

    #[test]
    fn toml_override_replaces_only_present_fields() {
        let base = ObsidianConfig {
            vault_root: Some("/from/db".into()),
            session_log_path: Some("/db/log.md".into()),
            ..ObsidianConfig::defaults()
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), r#"vault_root = "/from/toml""#).unwrap();
        let merged = apply_toml_override(base, tmp.path());
        assert_eq!(merged.vault_root.as_deref(), Some("/from/toml"));
        assert_eq!(merged.session_log_path.as_deref(), Some("/db/log.md"));
    }

    #[test]
    fn missing_toml_is_no_op() {
        let base = ObsidianConfig {
            vault_root: Some("/from/db".into()),
            ..ObsidianConfig::defaults()
        };
        let merged = apply_toml_override(base, Path::new("/nonexistent/path.toml"));
        assert_eq!(merged.vault_root.as_deref(), Some("/from/db"));
    }

    #[test]
    fn normalize_appends_md_when_no_extension() {
        assert_eq!(normalize_md_path("/vault/Inbox"), "/vault/Inbox.md");
        assert_eq!(normalize_md_path("Inbox"), "Inbox.md");
    }

    #[test]
    fn normalize_leaves_existing_md_alone() {
        assert_eq!(normalize_md_path("/vault/Inbox.md"), "/vault/Inbox.md");
    }

    #[test]
    fn normalize_leaves_other_extensions_alone() {
        assert_eq!(normalize_md_path("/vault/notes.txt"), "/vault/notes.txt");
        assert_eq!(normalize_md_path("/vault/cfg.yml"), "/vault/cfg.yml");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_md_path("  /vault/Inbox  "), "/vault/Inbox.md");
    }

    #[test]
    fn normalize_empty_is_empty() {
        assert_eq!(normalize_md_path(""), "");
        assert_eq!(normalize_md_path("   "), "");
    }

    #[test]
    fn normalize_file_channels_only_touches_file_paths() {
        let mut cfg = ObsidianConfig {
            quick_capture_path: Some("/vault/Inbox".into()),
            session_log_path: Some("/vault/log".into()),
            moc_path: Some("/vault/MOCs".into()),
            templates_path: Some("/vault/Templates".into()),
            ..ObsidianConfig::defaults()
        };
        normalize_file_channels(&mut cfg);
        assert_eq!(cfg.quick_capture_path.as_deref(), Some("/vault/Inbox.md"));
        assert_eq!(cfg.session_log_path.as_deref(), Some("/vault/log.md"));
        assert_eq!(cfg.moc_path.as_deref(), Some("/vault/MOCs"));
        assert_eq!(cfg.templates_path.as_deref(), Some("/vault/Templates"));
    }

    #[test]
    fn expand_home_replaces_tilde_prefix() {
        let home = dirs::home_dir().unwrap();
        let home_str = home.to_string_lossy();
        assert_eq!(
            expand_home("~/Documents/Vault"),
            format!("{home_str}/Documents/Vault")
        );
        assert_eq!(expand_home("~"), home_str.to_string());
    }

    #[test]
    fn expand_home_passes_absolute_paths_through() {
        assert_eq!(expand_home("/already/absolute"), "/already/absolute");
        assert_eq!(expand_home("~name/foo"), "~name/foo");
    }

    // The normalize pipeline routes through `resolve_case_insensitive`, which is
    // Linux-case-sensitivity machinery: on Windows it rebuilds the path with native
    // `\` separators, so the `/`-joined expectations below are Unix-specific.
    #[cfg(unix)]
    #[test]
    fn normalize_expands_tilde_in_all_path_fields() {
        let home = dirs::home_dir().unwrap();
        let home_str = home.to_string_lossy().into_owned();
        let mut cfg = ObsidianConfig {
            vault_root: Some("~/Documents/Obsidian23".into()),
            templates_path: Some("~/Documents/Obsidian23".into()),
            quick_capture_path: Some("~/Documents/Obsidian23/Inbox".into()),
            ..ObsidianConfig::defaults()
        };
        normalize_file_channels(&mut cfg);
        assert_eq!(
            cfg.vault_root.as_deref(),
            Some(format!("{home_str}/Documents/Obsidian23").as_str())
        );
        assert_eq!(
            cfg.templates_path.as_deref(),
            Some(format!("{home_str}/Documents/Obsidian23").as_str())
        );
        assert_eq!(
            cfg.quick_capture_path.as_deref(),
            Some(format!("{home_str}/Documents/Obsidian23/Inbox.md").as_str())
        );
    }

    #[cfg(unix)] // case-correction is a no-op on Windows's case-insensitive FS
    #[test]
    fn case_insensitive_corrects_unique_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Documents").join("Obsidian23")).unwrap();
        let typed = dir
            .path()
            .join("documents")
            .join("obsidian23")
            .to_string_lossy()
            .into_owned();
        let expected = dir
            .path()
            .join("Documents")
            .join("Obsidian23")
            .to_string_lossy()
            .into_owned();
        assert_eq!(resolve_case_insensitive(&typed), expected);
    }

    #[test]
    fn case_insensitive_keeps_exact_match_untouched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Vault")).unwrap();
        let exact = dir.path().join("Vault").to_string_lossy().into_owned();
        assert_eq!(resolve_case_insensitive(&exact), exact);
    }

    #[test]
    fn case_insensitive_ambiguous_match_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Vault")).unwrap();
        std::fs::create_dir_all(dir.path().join("vault")).unwrap();
        let typed = dir.path().join("VAULT").to_string_lossy().into_owned();
        assert_eq!(resolve_case_insensitive(&typed), typed);
    }

    #[cfg(unix)] // case-correction is a no-op on Windows's case-insensitive FS
    #[test]
    fn case_insensitive_leaves_nonexistent_tail_as_typed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Vault")).unwrap();
        let typed = dir
            .path()
            .join("vault")
            .join("Future Note.md")
            .to_string_lossy()
            .into_owned();
        let expected = dir
            .path()
            .join("Vault")
            .join("Future Note.md")
            .to_string_lossy()
            .into_owned();
        assert_eq!(resolve_case_insensitive(&typed), expected);
    }

    #[test]
    fn case_insensitive_relative_path_untouched() {
        assert_eq!(resolve_case_insensitive("relative/path"), "relative/path");
    }

    #[test]
    fn malformed_toml_falls_back_to_base() {
        let base = ObsidianConfig {
            vault_root: Some("/from/db".into()),
            ..ObsidianConfig::defaults()
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "this is not = valid toml [[[").unwrap();
        let merged = apply_toml_override(base, tmp.path());
        assert_eq!(merged.vault_root.as_deref(), Some("/from/db"));
    }
}
