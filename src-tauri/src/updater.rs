//! In-app update flow.
//!
//! Source-aware on purpose: cluihud must never trigger a sudo prompt, so
//! `.deb` installs only ever stage the new package in `~/Downloads/` and
//! defer the elevation to the user's own package-manager UI. AppImage
//! signed auto-install is a future addition; today both paths share the
//! same download-and-reveal flow.

use std::env;
use std::path::PathBuf;

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
        "/usr/bin/cluihud" | "/usr/local/bin/cluihud"
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

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/Mufdi/nergal/releases/latest";

/// GitHub rejects unauthenticated requests without a User-Agent header.
const USER_AGENT: &str = concat!("cluihud-updater/", env!("CARGO_PKG_VERSION"));

/// Distinct from `check_app_update`: this is always-on "what's new in
/// what you have", so dev builds and up-to-date installs still see a
/// changelog without having to trigger an update check. Returns
/// `Ok(None)` when no release exists for the current version yet.
#[tauri::command]
pub async fn get_current_release_notes() -> Result<Option<CurrentReleaseInfo>, String> {
    let current = env!("CARGO_PKG_VERSION");
    let url = format!(
        "https://api.github.com/repos/Mufdi/nergal/releases/tags/v{current}"
    );
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

/// Numeric-segments-only on purpose: cluihud's tag space is `0.1.x`.
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

#[tauri::command]
pub async fn download_app_update(url: String, filename: String) -> Result<String, String> {
    // Host allow-list closes a path that would otherwise let the
    // frontend coerce this command into fetching arbitrary URLs.
    if !url.starts_with("https://github.com/") && !url.starts_with("https://objects.githubusercontent.com/") {
        return Err("refusing to download outside github.com hosts".into());
    }
    // Filename comes from the frontend; reject anything that would
    // escape the Downloads dir.
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("invalid filename".into());
    }
    let dir = resolve_downloads_dir()?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create downloads dir: {e}"))?;
    let target = dir.join(&filename);
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

/// `xdg-open` (vs. nautilus/dolphin/thunar directly) so the user's
/// configured default file manager wins, regardless of DE.
#[tauri::command]
pub fn reveal_in_downloads(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let dir = p
        .parent()
        .ok_or_else(|| "path has no parent".to_string())?;
    std::process::Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(())
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
        let exe = "/home/user/projects/cluihud/src-tauri/target/release/cluihud";
        assert!(exe.contains("/target/release/"));
    }

    #[test]
    fn download_rejects_path_traversal() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(download_app_update(
            "https://github.com/x/x/releases/foo.deb".into(),
            "../../etc/passwd".into(),
        ));
        assert!(err.is_err());
    }

    #[test]
    fn download_rejects_off_host_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(download_app_update(
            "https://evil.example.com/payload.deb".into(),
            "payload.deb".into(),
        ));
        assert!(err.is_err());
    }
}
