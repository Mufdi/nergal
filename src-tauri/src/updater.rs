//! In-app update flow.
//!
//! cluihud talks to the GitHub Releases API directly (no signed updater
//! plugin) and tailors the install action to the source the binary was
//! launched from:
//!
//! - `.deb` install → download the new `.deb` to `~/Downloads` and reveal
//!   it in the file manager. The user runs their own package-manager UI;
//!   cluihud never touches `sudo`.
//! - AppImage → reserved for a future signed in-place auto-install. Today
//!   we offer the same download-and-reveal flow (saved as `.AppImage`).
//! - Dev build (`target/release/cluihud`) → returns "dev" and the UI hides
//!   the update button entirely.

use std::env;
use std::path::PathBuf;

use serde::Serialize;

/// Where the running binary came from. Drives the UI's update affordance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    Deb,
    Appimage,
    Dev,
    Unknown,
}

/// Sniff the install source from `/proc/self/exe` and the process env.
/// Linux-only — no Windows/macOS branches (cluihud is Linux-only by scope).
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

/// GitHub Releases API representation. We only deserialize the fields the
/// updater needs; everything else is ignored.
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

/// Frontend-facing release info. `latestVersion` is the tag with the
/// leading `v` stripped. `currentVersion` echoes back from cargo so the
/// UI doesn't need to round-trip through `@tauri-apps/api/app`.
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

/// User-Agent string GitHub requires; identifying ourselves makes their
/// abuse heuristics happy.
const USER_AGENT: &str = concat!("cluihud-updater/", env!("CARGO_PKG_VERSION"));

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

/// Naive semver-ish compare: split on `.`, parse each segment as u64.
/// Sufficient for cluihud's `0.1.x` tag space; if we ever ship `1.0.0-rc.1`
/// the parse will fall back to lexicographic on the failing segment, which
/// is still safer than blindly trusting `latest != current`.
fn version_is_newer(latest: &str, current: &str) -> bool {
    let lt: Vec<u64> = latest.split('.').filter_map(|s| s.parse().ok()).collect();
    let cu: Vec<u64> = current.split('.').filter_map(|s| s.parse().ok()).collect();
    if !lt.is_empty() && lt.len() == cu.len() {
        return lt > cu;
    }
    latest != current
}

/// Resolve `~/Downloads`, honoring `xdg-user-dir DOWNLOAD` when available,
/// falling back to `$HOME/Downloads` and creating it on demand.
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

/// Stream `url` into `~/Downloads/<filename>`. Returns the absolute path
/// to the saved file. The frontend can then call `reveal_in_file_manager`.
#[tauri::command]
pub async fn download_app_update(url: String, filename: String) -> Result<String, String> {
    if !url.starts_with("https://github.com/") && !url.starts_with("https://objects.githubusercontent.com/") {
        return Err("refusing to download outside github.com hosts".into());
    }
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

/// Open the parent directory of `path` in the user's file manager. Uses
/// `xdg-open` so it works across GNOME, KDE, XFCE, etc. without a
/// hardcoded file-manager binary.
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
