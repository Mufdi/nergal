//! In-app update flow.
//!
//! Source-aware on purpose: nergal must never trigger a sudo prompt, so
//! `.deb` installs only ever stage the new package in `~/Downloads/` and
//! defer the elevation to the user's own package-manager UI. The AppImage
//! branch IS the live signed auto-install path (`tauri-plugin-updater`
//! `downloadAndInstall`); the Windows installer shares that same OS-agnostic
//! action. `.deb`/`.app` stay download-and-reveal (sudo / Gatekeeper).

use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri_plugin_opener::OpenerExt;

/// Drives the UI's update affordance — see module doc for the per-source
/// policy. `Dev` exists so dev builds get a warning banner instead of a
/// useless download button.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    Deb,
    Appimage,
    MacApp,
    // Only constructed under #[cfg(windows)] (installed-build verdict); the
    // serde-driven frontend uses it on every target, but dead-code analysis
    // can't see that, so silence it off-Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    Windows,
    Dev,
    Unknown,
}

/// Pure classifier so the `.app/Contents/MacOS/` branch is unit-testable
/// without the test binary's real `current_exe()` path (which is always
/// under `target/` in CI).
fn install_source_for_path(exe_str: &str, appimage_env: bool) -> InstallSource {
    if appimage_env {
        return InstallSource::Appimage;
    }
    if exe_str.ends_with(".AppImage") {
        return InstallSource::Appimage;
    }
    // Matches /Applications/Nergal.app/Contents/MacOS/nergal and ~/Applications/...
    if exe_str.contains(".app/Contents/MacOS/") {
        return InstallSource::MacApp;
    }
    if matches!(exe_str, "/usr/bin/nergal" | "/usr/local/bin/nergal") {
        return InstallSource::Deb;
    }
    // Backslash forms classify a Windows dev build as Dev too — kept
    // unconditional (a pure string check) so the Linux `cargo test` covers it;
    // no CI runs Windows `cargo test`.
    if exe_str.contains("/target/release/")
        || exe_str.contains("/target/debug/")
        || exe_str.contains("\\target\\release\\")
        || exe_str.contains("\\target\\debug\\")
    {
        return InstallSource::Dev;
    }
    // An installed Windows build (NSIS/MSI) lands outside all the prefixes
    // above; the installed→Windows verdict is the only half gated to the
    // Windows target, so non-Windows hosts still fall through to Unknown.
    #[cfg(windows)]
    {
        InstallSource::Windows
    }
    #[cfg(not(windows))]
    {
        InstallSource::Unknown
    }
}

pub fn detect_install_source() -> InstallSource {
    let appimage_env = env::var_os("APPIMAGE").is_some();
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => return InstallSource::Unknown,
    };
    install_source_for_path(&exe.to_string_lossy(), appimage_env)
}

#[tauri::command]
pub fn get_install_source() -> InstallSource {
    detect_install_source()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealth {
    pub missing_binaries: Vec<String>,
}

/// Surfaces installs that skipped dependency resolution (`dpkg -i` without
/// apt) in the About section, instead of letting reveal/clone/update flows
/// fail silently at first use.
#[tauri::command]
pub fn check_system_health() -> SystemHealth {
    let missing_binaries = ["git"]
        .into_iter()
        .filter(|bin| which::which(bin).is_err())
        .map(str::to_string)
        .collect();
    SystemHealth { missing_binaries }
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// `currentVersion` echoes back from cargo here so the UI doesn't have
/// to round-trip through `@tauri-apps/api/app` to compare against latest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
    pub deb_asset_url: Option<String>,
    pub deb_asset_size: Option<u64>,
    pub appimage_asset_url: Option<String>,
    pub appimage_asset_size: Option<u64>,
    pub dmg_asset_url: Option<String>,
    pub dmg_asset_size: Option<u64>,
}

const GITHUB_RELEASES_URL: &str = "https://api.github.com/repos/Mufdi/nergal/releases/latest";

/// GitHub rejects unauthenticated requests without a User-Agent header.
const USER_AGENT: &str = concat!("nergal-updater/", env!("CARGO_PKG_VERSION"));

/// Distinct from `check_app_update`: this is always-on "what's new in
/// what you have", so dev builds and up-to-date installs still see a
/// changelog without having to trigger an update check. Returns
/// `Ok(None)` when no release exists for the current version yet.
#[tauri::command]
pub async fn get_current_release_notes() -> Result<Option<CurrentReleaseInfo>, String> {
    let current = env!("CARGO_PKG_VERSION");
    let url = format!("https://api.github.com/repos/Mufdi/nergal/releases/tags/v{current}");
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("github release lookup: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("github release returned {}", resp.status()));
    }
    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("github release body: {e}"))?;
    Ok(Some(CurrentReleaseInfo {
        version: current.to_string(),
        notes: release.body,
        release_url: release.html_url,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentReleaseInfo {
    pub version: String,
    pub notes: Option<String>,
    pub release_url: String,
}

#[tauri::command]
pub async fn check_app_update() -> Result<UpdateCheckResult, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(GITHUB_RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("github releases: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("github releases returned {}", resp.status()));
    }
    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("github releases body: {e}"))?;
    if release.draft || release.prerelease {
        return Err("latest release is a draft/prerelease".into());
    }
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let has_update = version_is_newer(&latest, &current);
    let deb = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".deb") && a.name.contains("amd64"));
    let appimage = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".AppImage"));
    let dmg = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".dmg") && a.name.contains("aarch64"));
    Ok(UpdateCheckResult {
        current_version: current,
        latest_version: latest,
        has_update,
        release_url: release.html_url,
        release_notes: release.body,
        deb_asset_url: deb.map(|a| a.browser_download_url.clone()),
        deb_asset_size: deb.map(|a| a.size),
        appimage_asset_url: appimage.map(|a| a.browser_download_url.clone()),
        appimage_asset_size: appimage.map(|a| a.size),
        dmg_asset_url: dmg.map(|a| a.browser_download_url.clone()),
        dmg_asset_size: dmg.map(|a| a.size),
    })
}

/// Numeric-segments-only on purpose: nergal's tag space is `0.1.x`.
/// A future `1.0.0-rc.1` will fail the numeric parse and fall back to
/// the string-inequality branch, which is still safer than blindly
/// trusting `latest != current` (mis-ordered tags wouldn't false-flag
/// as updates).
fn version_is_newer(latest: &str, current: &str) -> bool {
    let lt: Vec<u64> = latest.split('.').filter_map(|s| s.parse().ok()).collect();
    let cu: Vec<u64> = current.split('.').filter_map(|s| s.parse().ok()).collect();
    if !lt.is_empty() && lt.len() == cu.len() {
        return lt > cu;
    }
    latest != current
}

/// Pure helper so unit tests can exercise the fallback chain without touching
/// the real `dirs` queries (which depend on the current user environment).
fn downloads_from(dl: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    dl.or_else(|| home.map(|h| h.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("/tmp/Downloads"))
}

/// `dirs::download_dir()` covers both Linux (respects xdg-user-dirs) and macOS
/// (`~/Downloads`); no subprocess needed.
fn resolve_downloads_dir() -> Result<PathBuf, String> {
    Ok(downloads_from(dirs::download_dir(), dirs::home_dir()))
}

/// Filename comes from the frontend; reject anything that would
/// escape the Downloads dir.
fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
    {
        return Err("invalid filename".into());
    }
    Ok(())
}

/// A staged download counts as complete only when its size matches the
/// release asset byte count — a partial file from an interrupted download
/// must not short-circuit the re-download.
async fn staged_download_complete(target: &Path, expected_size: Option<u64>) -> bool {
    match tokio::fs::metadata(target).await {
        Ok(meta) => meta.is_file() && expected_size.is_some_and(|s| meta.len() == s),
        Err(_) => false,
    }
}

/// Lets the UI land directly on the "downloaded" state when the asset was
/// already staged in a previous visit to the About section.
#[tauri::command]
pub async fn find_downloaded_update(
    filename: String,
    expected_size: Option<u64>,
) -> Result<Option<String>, String> {
    validate_filename(&filename)?;
    let target = resolve_downloads_dir()?.join(&filename);
    if staged_download_complete(&target, expected_size).await {
        Ok(Some(target.to_string_lossy().into_owned()))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn download_app_update(
    url: String,
    filename: String,
    expected_size: Option<u64>,
) -> Result<String, String> {
    // Host allow-list closes a path that would otherwise let the
    // frontend coerce this command into fetching arbitrary URLs.
    if !url.starts_with("https://github.com/")
        && !url.starts_with("https://objects.githubusercontent.com/")
    {
        return Err("refusing to download outside github.com hosts".into());
    }
    validate_filename(&filename)?;
    let dir = resolve_downloads_dir()?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create downloads dir: {e}"))?;
    let target = dir.join(&filename);
    if staged_download_complete(&target, expected_size).await {
        return Ok(target.to_string_lossy().into_owned());
    }
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download returned {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("download body: {e}"))?;
    tokio::fs::write(&target, &bytes)
        .await
        .map_err(|e| format!("save: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

/// Resolve the default application id for a MIME type via `xdg-mime`, or
/// `None` when no handler is registered (empty output) / the query fails.
#[cfg(target_os = "linux")]
fn default_app_for_mime(mime: &str) -> Option<String> {
    let out = std::process::Command::new("xdg-mime")
        .args(["query", "default", mime])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let app = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!app.is_empty()).then_some(app)
}

/// Open the app log for diagnostics. The log only exists when launched under
/// journald redirect (the GNOME launcher path — see
/// `redirect_journald_stdio_to_logfile`); a terminal/dev run logs to stderr,
/// so report a clear error instead of opening a non-existent file.
#[tauri::command]
pub fn open_log_file(app: tauri::AppHandle) -> Result<(), String> {
    let log_path = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("nergal")
        .join("nergal.log");
    if !log_path.is_file() {
        return Err(format!(
            "no log file at {} (only written when launched from the app launcher)",
            log_path.display()
        ));
    }
    open_log_path(&app, &log_path)
}

/// On Linux `xdg-open`/the opener plugin resolve the handler by EXTENSION:
/// `.log` maps to `text/x-log`, which has no registered default app on a stock
/// desktop, so the call exits 0 without opening anything (the v0.3.0 "0 action,
/// 0 feedback" bug — and the platform-desktop regression when this was replaced
/// by a bare `opener().open_path`). Launch the `text/plain` default app
/// directly — the content IS plain text — to bypass the extension mapping,
/// falling back to revealing the containing folder (directories always have a
/// file-manager handler) so the log stays reachable on desktops without
/// `gtk-launch` or a `text/plain` default.
#[cfg(target_os = "linux")]
fn open_log_path(_app: &tauri::AppHandle, log_path: &Path) -> Result<(), String> {
    if let Some(app_id) = default_app_for_mime("text/plain")
        && std::process::Command::new("gtk-launch")
            .arg(&app_id)
            .arg(log_path)
            .spawn()
            .is_ok()
    {
        return Ok(());
    }
    let dir = log_path
        .parent()
        .ok_or_else(|| "log path has no parent".to_string())?;
    std::process::Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(())
}

/// macOS LaunchServices resolves `.log` to a real handler (Console/TextEdit),
/// so the plugin's `open_path` works without the Linux workaround.
#[cfg(not(target_os = "linux"))]
fn open_log_path(app: &tauri::AppHandle, log_path: &Path) -> Result<(), String> {
    app.opener()
        .open_path(log_path.to_string_lossy().as_ref(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn read_log_tail(n: usize) -> String {
    let log_path = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("nergal")
        .join("nergal.log");
    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        }
        Err(_) => "(no log file — only written when launched from the app launcher)".to_string(),
    }
}

/// Build a diagnostics bundle (version, install source, OS, recent log tail)
/// the user can paste into a bug report. Best-effort: missing pieces render as
/// "unknown" rather than failing.
#[tauri::command]
pub fn collect_diagnostics() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let source = format!("{:?}", detect_install_source()).to_lowercase();
    let os = crate::platform_proc::os_name().unwrap_or_else(|| "unknown".into());
    let kernel = crate::platform_proc::kernel_version().unwrap_or_else(|| "unknown".into());
    let log_tail = read_log_tail(150);
    format!(
        "Nergal diagnostics\nVersion: {version}\nInstall source: {source}\nOS: {os}\nKernel: {kernel}\n\n---- log tail (last 150 lines) ----\n{log_tail}"
    )
}

#[tauri::command]
pub async fn reveal_in_downloads(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("downloaded file no longer exists".into());
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_handles_patch_bump() {
        assert!(version_is_newer("0.1.3", "0.1.2"));
        assert!(!version_is_newer("0.1.2", "0.1.3"));
        assert!(!version_is_newer("0.1.2", "0.1.2"));
    }

    #[test]
    fn version_compare_handles_minor_bump() {
        assert!(version_is_newer("0.2.0", "0.1.9"));
        assert!(!version_is_newer("0.1.10", "0.2.0"));
    }

    #[test]
    fn install_source_recognizes_dev_build_path() {
        assert_eq!(
            install_source_for_path(
                "/home/user/projects/nergal/src-tauri/target/release/nergal",
                false,
            ),
            InstallSource::Dev,
        );
    }

    #[test]
    fn install_source_recognizes_mac_app_bundle_path() {
        assert_eq!(
            install_source_for_path("/Applications/Nergal.app/Contents/MacOS/nergal", false),
            InstallSource::MacApp,
        );
        // User Applications folder variant
        assert_eq!(
            install_source_for_path(
                "/Users/user/Applications/Nergal.app/Contents/MacOS/nergal",
                false,
            ),
            InstallSource::MacApp,
        );
    }

    #[test]
    fn install_source_recognizes_windows_dev_build_path() {
        // Backslash dev path → Dev on any host (pure string classifier, so the
        // Linux job covers the Windows dev-detection half).
        assert_eq!(
            install_source_for_path(
                "C:\\Users\\dev\\nergal\\src-tauri\\target\\release\\nergal.exe",
                false,
            ),
            InstallSource::Dev,
        );
    }

    // The installed→Windows verdict only compiles on the Windows target, so this
    // runs on the user's machine / a future Windows test runner, not Linux CI.
    #[cfg(windows)]
    #[test]
    fn install_source_installed_windows_is_windows() {
        assert_eq!(
            install_source_for_path("C:\\Program Files\\Nergal\\nergal.exe", false),
            InstallSource::Windows,
        );
    }

    #[test]
    fn install_source_classifier_regression() {
        // Existing sources must be unaffected by the new MacApp probe.
        assert_eq!(
            install_source_for_path("/usr/bin/nergal", false),
            InstallSource::Deb,
        );
        assert_eq!(
            install_source_for_path("/usr/bin/Nergal_0.4.1_amd64.AppImage", false),
            InstallSource::Appimage,
        );
        // APPIMAGE env var takes priority regardless of exe path
        assert_eq!(
            install_source_for_path("/usr/bin/nergal", true),
            InstallSource::Appimage,
        );
        assert_eq!(
            install_source_for_path(
                "/home/user/projects/nergal/src-tauri/target/debug/nergal",
                false,
            ),
            InstallSource::Dev,
        );
        assert_eq!(
            install_source_for_path("/opt/nergal/nergal", false),
            InstallSource::Unknown,
        );
    }

    #[test]
    fn download_rejects_path_traversal() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(download_app_update(
            "https://github.com/x/x/releases/foo.deb".into(),
            "../../etc/passwd".into(),
            None,
        ));
        assert!(err.is_err());
    }

    #[test]
    fn download_rejects_off_host_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(download_app_update(
            "https://evil.example.com/payload.deb".into(),
            "payload.deb".into(),
            None,
        ));
        assert!(err.is_err());
    }

    #[test]
    fn staged_download_requires_exact_size_match() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Nergal_0.2.0_amd64.deb");
        std::fs::write(&path, b"12345").unwrap();
        assert!(rt.block_on(staged_download_complete(&path, Some(5))));
        // Partial download (size mismatch) must trigger a re-download.
        assert!(!rt.block_on(staged_download_complete(&path, Some(9999))));
        // Unknown asset size: completeness can't be proven, so re-download.
        assert!(!rt.block_on(staged_download_complete(&path, None)));
        assert!(!rt.block_on(staged_download_complete(
            &dir.path().join("missing.deb"),
            Some(5)
        )));
    }

    #[test]
    fn filename_validator_rejects_escapes() {
        assert!(validate_filename("Nergal_0.2.0_amd64.deb").is_ok());
        assert!(validate_filename("").is_err());
        assert!(validate_filename("a/b.deb").is_err());
        assert!(validate_filename("a\\b.deb").is_err());
        assert!(validate_filename("..deb..").is_err());
    }

    #[test]
    fn downloads_from_uses_dl_dir_when_available() {
        assert_eq!(
            downloads_from(Some(PathBuf::from("/d")), Some(PathBuf::from("/h"))),
            PathBuf::from("/d")
        );
    }

    #[test]
    fn downloads_from_falls_back_to_home_downloads() {
        assert_eq!(
            downloads_from(None, Some(PathBuf::from("/h"))),
            PathBuf::from("/h/Downloads")
        );
    }

    #[test]
    fn downloads_from_falls_back_to_tmp_when_both_none() {
        assert_eq!(downloads_from(None, None), PathBuf::from("/tmp/Downloads"));
    }
}
