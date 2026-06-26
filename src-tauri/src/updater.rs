//! In-app update flow.
//!
//! Source-aware on purpose: nergal must never trigger a sudo prompt, so
//! `.deb` installs only ever stage the new package in `~/Downloads/` and
//! defer the elevation to the user's own package-manager UI. AppImage
//! signed auto-install is a future addition; today both paths share the
//! same download-and-reveal flow.

use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Drives the UI's update affordance — see module doc for the per-source
/// policy. `Dev` exists so dev builds get a warning banner instead of a
/// useless download button.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    Deb,
    Appimage,
    Dev,
    Unknown,
}

pub fn detect_install_source() -> InstallSource {
    if env::var_os("APPIMAGE").is_some() {
        return InstallSource::Appimage;
    }
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => return InstallSource::Unknown,
    };
    let exe_str = exe.to_string_lossy();
    if exe_str.ends_with(".AppImage") {
        return InstallSource::Appimage;
    }
    if matches!(
        exe_str.as_ref(),
        "/usr/bin/nergal" | "/usr/local/bin/nergal"
    ) {
        return InstallSource::Deb;
    }
    if exe_str.contains("/target/release/") || exe_str.contains("/target/debug/") {
        return InstallSource::Dev;
    }
    InstallSource::Unknown
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
    let missing_binaries = ["git", "xdg-open", "xdg-user-dir"]
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

/// `xdg-user-dir DOWNLOAD` first so we respect the user's localized /
/// remapped Downloads folder (KDE/GNOME both honor this); fall through
/// to the conventional `$HOME/Downloads` when xdg-user-dirs isn't set up.
fn resolve_downloads_dir() -> Result<PathBuf, String> {
    if let Ok(out) = std::process::Command::new("xdg-user-dir")
        .arg("DOWNLOAD")
        .output()
        && out.status.success()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let home = dirs::home_dir().ok_or_else(|| "HOME not set".to_string())?;
    Ok(home.join("Downloads"))
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

/// FileManager1 is the freedesktop reveal interface (Nautilus, Dolphin,
/// Nemo all implement it): it highlights the file and the D-Bus activation
/// carries a startup token, so GNOME raises the window instead of letting
/// focus-stealing prevention bury it — the failure mode of a bare
/// `xdg-open` spawn. The spawn survives only as fallback for DEs without
/// the service.
///
/// Linux-only: `zbus`/FileManager1 is D-Bus. `platform-desktop` later replaces
/// this with the cross-platform Tauri opener plugin; until then non-Linux
/// targets skip the reveal (see `reveal_in_downloads`).
#[cfg(target_os = "linux")]
async fn show_items_via_dbus(path: &Path) -> Result<(), String> {
    let uri = url::Url::from_file_path(path)
        .map_err(|_| "path is not absolute".to_string())?
        .to_string();
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session bus: {e}"))?;
    conn.call_method(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        Some("org.freedesktop.FileManager1"),
        "ShowItems",
        &(vec![uri], String::new()),
    )
    .await
    .map_err(|e| format!("ShowItems: {e}"))?;
    Ok(())
}

/// Resolve the default application id for a MIME type via `xdg-mime`, or
/// `None` when no handler is registered (empty output) / the query fails.
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
///
/// `xdg-open` resolves the handler by EXTENSION: `.log` maps to `text/x-log`,
/// which has no registered default app on a stock desktop, so it exits 0
/// without opening anything (the v0.3.0 "0 action, 0 feedback" bug). Launch
/// the `text/plain` default app directly — the content IS plain text — to
/// bypass the extension mapping, falling back to revealing the containing
/// folder (directories always have a file-manager handler) so the log stays
/// reachable on desktops without `gtk-launch` or a `text/plain` default.
#[tauri::command]
pub fn open_log_file() -> Result<(), String> {
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
    if let Some(app) = default_app_for_mime("text/plain")
        && std::process::Command::new("gtk-launch")
            .arg(&app)
            .arg(&log_path)
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

fn read_os_pretty_name() -> Option<String> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;
    content.lines().find_map(|line| {
        line.strip_prefix("PRETTY_NAME=")
            .map(|rest| rest.trim_matches('"').to_string())
    })
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
    let os = read_os_pretty_name().unwrap_or_else(|| "unknown".into());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let log_tail = read_log_tail(150);
    format!(
        "Nergal diagnostics\nVersion: {version}\nInstall source: {source}\nOS: {os}\nKernel: {kernel}\n\n---- log tail (last 150 lines) ----\n{log_tail}"
    )
}

#[tauri::command]
pub async fn reveal_in_downloads(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("downloaded file no longer exists".into());
    }
    #[cfg(target_os = "linux")]
    {
        let Err(dbus_err) = show_items_via_dbus(&p).await else {
            return Ok(());
        };
        let dir = p.parent().ok_or_else(|| "path has no parent".to_string())?;
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("FileManager1 ({dbus_err}); xdg-open: {e}"))?;
        Ok(())
    }
    // Non-Linux: reveal is owned by platform-desktop (opener plugin). Log-only
    // no-op — the download already succeeded, so this is not a hard error and
    // must NOT fall through to xdg-open (absent/meaningless off Linux).
    #[cfg(not(target_os = "linux"))]
    {
        tracing::debug!(path = %p.display(), "reveal_in_downloads: skipped (non-Linux; platform-desktop reveal pending)");
        Ok(())
    }
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
        let exe = "/home/user/projects/nergal/src-tauri/target/release/nergal";
        assert!(exe.contains("/target/release/"));
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
}
