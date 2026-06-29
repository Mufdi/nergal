//! Single source for CC's `plansDirectory`. Both the plan watcher startup
//! (`lib.rs`) and `list_session_plans` must agree — they diverged before this
//! module existed and that mismatch silently produced empty panels for users
//! with a custom `plansDirectory`. Pure / sync — safe to call per-request.

use std::path::{Path, PathBuf};

pub fn resolve_cc_plans_directory(cwd: &Path) -> PathBuf {
    resolve_with_home(cwd, dirs::home_dir().as_deref())
}

fn resolve_with_home(cwd: &Path, home: Option<&Path>) -> PathBuf {
    let configured = read_plans_directory_from_layers(cwd, home)
        .map(|raw| normalize_to_absolute(&raw, cwd, home));
    configured.unwrap_or_else(|| cwd.join(".claude").join("plans"))
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
