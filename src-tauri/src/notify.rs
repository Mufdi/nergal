//! Shared desktop-notification helper.
//!
//! The Linux branch keeps `notify-send` as the primary path because
//! `tauri_plugin_notification`'s `.show()` was observed to return `Ok(())`
//! while displaying nothing under WebKitGTK (Decision 3/3a). Silent failure
//! is NOT runtime-detectable, so the branch is chosen at build time by the
//! task 6.0 empirical gate. Until a human GUI session confirms the plugin
//! displays reliably on this build's WebKitGTK, `notify-send` stays primary.
//!
//! macOS always uses the plugin (no `notify-send` there).
//!
//! Task 6.0 observation: DEFERRED — could not run `pnpm tauri dev` (no
//! display in the builder session). Conservative/non-regressive branch taken.

/// Fire a desktop notification. Non-fatal: failures are `tracing::warn!`-ed
/// and callers do not need to inspect the result.
pub fn send(app: &tauri::AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        // notify-send is the primary Linux path; the plugin is NOT used here
        // because its Ok(()) is indistinguishable from a silent no-display
        // under WebKitGTK (Decision 3a). If this build ever empirically
        // confirms the plugin displays, replace this block with the macOS
        // branch below.
        let _ = app;
        if let Err(e) = std::process::Command::new("notify-send")
            .arg("--app-name=nergal")
            .arg("--expire-time=4000")
            .arg("--urgency=normal")
            .arg(title)
            .arg(body)
            .spawn()
        {
            tracing::warn!("notify-send failed: {e}");
        }
    }

    // macOS and Windows both use the Tauri notification plugin. The WebKitGTK
    // silent-no-display caveat (Decision 3a) is Linux-only; on macOS (Cocoa) and
    // Windows (WinRT toasts via the bundle AUMID `com.nergal.app`) the plugin
    // displays reliably.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use tauri_plugin_notification::NotificationExt;
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            tracing::warn!("notification plugin failed: {e}");
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        tracing::warn!(
            title,
            body,
            "desktop notification not implemented on this platform"
        );
    }
}
