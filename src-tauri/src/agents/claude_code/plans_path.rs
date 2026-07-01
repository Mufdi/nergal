//! Single source for CC's `plansDirectory`. Both the plan watcher startup
//! (`lib.rs`) and `list_session_plans` must agree — they diverged before this
//! module existed and that mismatch silently produced empty panels for users
//! with a custom `plansDirectory`. Pure / sync — safe to call per-request.

use std::path::{Path, PathBuf};

pub fn resolve_cc_plans_directory(cwd: &Path) -> PathBuf {
    resolve_with_home(cwd, dirs::home_dir().as_deref())
}

fn resolve_with_home(cwd: &Path, home: Option<&Path>) -> PathBuf {
    if let Some(raw) = read_plans_directory_from_layers(cwd, home) {
        return normalize_to_absolute(&raw, cwd, home);
    }
    // No explicit plansDirectory — fall back to CC's default. Older CC wrote to
    // the project-local `<cwd>/.claude/plans`; modern CC writes to the
    // home-global `~/.claude/plans` (the only location that exists on Windows).
    // Prefer the cwd-local dir when it already exists (keeps established project
    // layouts intact), else the home-global dir when IT exists, else the
    // cwd-local default for a fresh project (the watcher creates it on use).
    let cwd_local = cwd.join(".claude").join("plans");
    if cwd_local.exists() {
        return cwd_local;
    }
    if let Some(home) = home {
        let home_global = home.join(".claude").join("plans");
        if home_global.exists() {
            return home_global;
        }
    }
    cwd_local
}

/// Candidate directories to search for a workspace CWD, ordered most-specific
/// first. When the configured `plansDirectory` is relative, returns BOTH
/// `cwd/rel` and `home/rel` — CC may resolve relatives against home on Windows.
pub(crate) fn candidate_dirs(cwd: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    if let Some(raw) = read_plans_directory_from_layers(cwd, home) {
        let expanded = expand_tilde(&raw, home);
        if expanded.is_absolute() {
            return vec![expanded];
        }
        // Relative — search cwd-resolved (primary) and home-resolved (Windows fallback)
        let mut out = vec![cwd.join(&raw)];
        if let Some(h) = home {
            let hv = h.join(&raw);
            if !out.contains(&hv) {
                out.push(hv);
            }
        }
        return out;
    }
    vec![resolve_with_home(cwd, home)]
}

fn read_plans_directory_from_layers(cwd: &Path, home: Option<&Path>) -> Option<String> {
    let mut result: Option<String> = None;
    if let Some(home) = home
        && let Some(v) = read_plans_directory_from_file(&home.join(".claude/settings.json"))
    {
        result = Some(v);
    }
    if let Some(v) = read_plans_directory_from_file(&cwd.join(".claude/settings.json")) {
        result = Some(v);
    }
    if let Some(v) = read_plans_directory_from_file(&cwd.join(".claude/settings.local.json")) {
        result = Some(v);
    }
    result
}

fn read_plans_directory_from_file(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("plansDirectory")
        .and_then(|f| f.as_str())
        .map(str::to_owned)
}

fn normalize_to_absolute(raw: &str, cwd: &Path, home: Option<&Path>) -> PathBuf {
    let expanded = expand_tilde(raw, home);
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn expand_tilde(s: &str, home: Option<&Path>) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = home
    {
        return home.join(rest);
    }
    if s == "~"
        && let Some(home) = home
    {
        return home.to_path_buf();
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Layout {
        _root: tempfile::TempDir,
        home: PathBuf,
        cwd: PathBuf,
    }

    fn setup() -> Layout {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let cwd = root.path().join("project");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(cwd.join(".claude")).unwrap();
        Layout {
            _root: root,
            home,
            cwd,
        }
    }

    fn write_settings(path: &Path, json: &str) {
        std::fs::write(path, json).unwrap();
    }

    #[test]
    fn relative_plansdir_yields_cwd_and_home_candidates() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"plansDirectory":"notes/plans"}"#,
        );
        let candidates = candidate_dirs(&l.cwd, Some(&l.home));
        assert!(
            candidates.contains(&l.cwd.join("notes/plans")),
            "cwd candidate missing"
        );
        assert!(
            candidates.contains(&l.home.join("notes/plans")),
            "home candidate missing"
        );
    }

    #[test]
    fn missing_settings_returns_cwd_default() {
        let l = setup();
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.cwd.join(".claude/plans"));
    }

    #[test]
    fn relative_path_resolves_against_cwd() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"plansDirectory":"notes/plans"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.cwd.join("notes/plans"));
    }

    // `/custom/path/plans` is not an absolute path on Windows (no drive), so the
    // passthrough-vs-join branch differs by platform — assert the Unix form here.
    #[cfg(unix)]
    #[test]
    fn absolute_path_passes_through() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"plansDirectory":"/custom/path/plans"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, PathBuf::from("/custom/path/plans"));
    }

    #[test]
    fn tilde_expands_against_home() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"plansDirectory":"~/notes/plans"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.home.join("notes/plans"));
    }

    #[cfg(unix)] // asserts Unix-absolute plansDirectory literals
    #[test]
    fn project_local_overrides_home() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"plansDirectory":"/global"}"#,
        );
        write_settings(
            &l.cwd.join(".claude/settings.json"),
            r#"{"plansDirectory":"/project"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, PathBuf::from("/project"));
    }

    #[cfg(unix)] // asserts Unix-absolute plansDirectory literals
    #[test]
    fn settings_local_overrides_settings_in_cwd() {
        let l = setup();
        write_settings(
            &l.cwd.join(".claude/settings.json"),
            r#"{"plansDirectory":"/team"}"#,
        );
        write_settings(
            &l.cwd.join(".claude/settings.local.json"),
            r#"{"plansDirectory":"/personal"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, PathBuf::from("/personal"));
    }

    #[test]
    fn falls_back_to_home_global_when_cwd_local_absent() {
        // Modern CC / Windows: no plansDirectory, no project-local plans dir,
        // but the home-global ~/.claude/plans exists → resolve to home-global.
        let l = setup();
        std::fs::create_dir_all(l.home.join(".claude/plans")).unwrap();
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.home.join(".claude/plans"));
    }

    #[test]
    fn prefers_cwd_local_over_home_global_when_both_exist() {
        // Legacy project layout: an existing project-local plans dir wins over
        // the home-global default so established setups are untouched.
        let l = setup();
        std::fs::create_dir_all(l.cwd.join(".claude/plans")).unwrap();
        std::fs::create_dir_all(l.home.join(".claude/plans")).unwrap();
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.cwd.join(".claude/plans"));
    }

    #[test]
    fn corrupted_settings_falls_back_to_default() {
        let l = setup();
        write_settings(&l.home.join(".claude/settings.json"), "not valid json");
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.cwd.join(".claude/plans"));
    }

    #[test]
    fn settings_without_plans_directory_key_uses_default() {
        let l = setup();
        write_settings(
            &l.home.join(".claude/settings.json"),
            r#"{"defaultModel":"sonnet"}"#,
        );
        let got = resolve_with_home(&l.cwd, Some(&l.home));
        assert_eq!(got, l.cwd.join(".claude/plans"));
    }

    #[cfg(unix)] // asserts a Unix-absolute plansDirectory literal
    #[test]
    fn missing_home_falls_back_to_cwd_layers() {
        let l = setup();
        write_settings(
            &l.cwd.join(".claude/settings.json"),
            r#"{"plansDirectory":"/proj"}"#,
        );
        let got = resolve_with_home(&l.cwd, None);
        assert_eq!(got, PathBuf::from("/proj"));
    }
}
