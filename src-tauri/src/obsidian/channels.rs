use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::obsidian::config::ResolvedObsidianConfig;

pub struct QuickCaptureWriter;

impl QuickCaptureWriter {
    pub fn append(
        cfg: &ResolvedObsidianConfig,
        text: &str,
        tag: Option<&str>,
        project_path: Option<&str>,
    ) -> Result<Option<std::path::PathBuf>> {
        let Some(path_str) = cfg.quick_capture_path.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let path = Path::new(path_str);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {}", parent.display()))?;
        }
        let is_new = std::fs::metadata(path)
            .map(|m| m.len() == 0)
            .unwrap_or(true);
        // O_APPEND so concurrent sessions in the same workspace interleave
        // at line boundaries instead of byte-stomping each other.
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening quick_capture {}", path.display()))?;
        // Only the first write seeds the deep link, so captures don't repeat it.
        if is_new && let Some(repo) = project_path.filter(|p| !p.is_empty()) {
            let header = format!(
                "# Inbox\n\n[Open in Nergal](cluihud://open-workspace?path={})\n",
                crate::obsidian::bootstrap::encode_uri_component(repo)
            );
            file.write_all(header.as_bytes())
                .with_context(|| format!("writing inbox header to {}", path.display()))?;
        }
        let tag = tag.unwrap_or("nergal-inbox");
        let block = format!(
            "\n\n## {}\n{}\n\n#{}\n",
            human_timestamp_local(),
            text.trim_end(),
            tag
        );
        file.write_all(block.as_bytes())
            .with_context(|| format!("appending to {}", path.display()))?;
        file.sync_all().ok();
        Ok(Some(path.to_path_buf()))
    }
}

/// #2 continuous session log. One file per workspace (`session_log_path` is a
/// path to a file, per F); sessions are appended as `## Session` blocks. All
/// writes are O_APPEND so concurrent sessions interleave at line boundaries.
pub struct SessionLogWriter;

impl SessionLogWriter {
    fn open(cfg: &ResolvedObsidianConfig) -> Result<Option<std::fs::File>> {
        let Some(path_str) = cfg.session_log_path.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let path = Path::new(path_str);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening session_log {}", path.display()))?;
        Ok(Some(file))
    }

    pub fn start_session(
        cfg: &ResolvedObsidianConfig,
        session_name: &str,
        agent_id: &str,
        model_name: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<()> {
        if let Some(p) = cfg.session_log_path.as_deref().filter(|s| !s.is_empty()) {
            rotate_if_large(Path::new(p));
        }
        let Some(mut file) = Self::open(cfg)? else {
            return Ok(());
        };
        let agent_line = match model_name {
            Some(m) if !m.is_empty() => format!("- Agent: {agent_id} ({m})"),
            _ => format!("- Agent: {agent_id}"),
        };
        let block = format!(
            "\n## Session \"{name}\" — {ts}\n{agent_line}\n- Cwd: {cwd}\n\n### Activity\n",
            name = session_name,
            ts = human_timestamp_local(),
            cwd = cwd.unwrap_or("?"),
        );
        file.write_all(block.as_bytes())?;
        file.sync_all().ok();
        Ok(())
    }

    pub fn append_event(cfg: &ResolvedObsidianConfig, line: &str) -> Result<()> {
        let Some(mut file) = Self::open(cfg)? else {
            return Ok(());
        };
        let entry = format!("- {} · {}\n", human_timestamp_local(), line);
        file.write_all(entry.as_bytes())?;
        Ok(())
    }

    pub fn end_session(
        cfg: &ResolvedObsidianConfig,
        model: Option<&str>,
        files: &[String],
        tasks_completed: usize,
    ) -> Result<()> {
        let Some(mut file) = Self::open(cfg)? else {
            return Ok(());
        };
        let files_line = if files.is_empty() {
            "- Files touched: 0".to_string()
        } else {
            let basenames: Vec<String> = files
                .iter()
                .map(|f| {
                    Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| f.clone())
                })
                .collect();
            format!(
                "- Files touched: {} ({})",
                files.len(),
                basenames.join(", ")
            )
        };
        let model_line = match model {
            Some(m) if !m.is_empty() => format!("- Model: {m}\n"),
            _ => String::new(),
        };
        let block = format!(
            "\n### Session ended at {ts}\n{model_line}{files_line}\n- Tasks completed: {tc}\n",
            ts = human_timestamp_local(),
            tc = tasks_completed,
        );
        file.write_all(block.as_bytes())?;
        file.sync_all().ok();
        Ok(())
    }
}

/// Keeps the file the user (and the agent) reads bounded; overflow rolls to an
/// archive sibling. ~128KB ≈ a comfortable single read, not a many-thousand-line wall.
const MAX_LOG_BYTES: u64 = 128 * 1024;

fn rotate_if_large(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let archive = archive_path(path);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&archive) {
        let _ = f.write_all(content.as_bytes());
    }
    let _ = std::fs::write(path, b"");
}

fn archive_path(path: &Path) -> std::path::PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("log");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("md");
    path.with_file_name(format!("{stem} (archive).{ext}"))
}

pub fn iso_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_iso_utc(secs)
}

/// ISO-8601 UTC for a known unix-seconds instant (e.g. a session's created_at).
pub fn iso_from_unix(secs: i64) -> String {
    format_iso_utc(secs)
}

pub fn human_timestamp_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_human_utc(secs)
}

pub fn human_timestamp_local() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_human_local(secs)
}

fn format_human_utc(secs: i64) -> String {
    let iso = format_iso_utc(secs);
    // iso is YYYY-MM-DDTHH:MM:SSZ — swap T → space and Z → " UTC" so the
    // header reads as a date the user actually parses at a glance.
    iso.replace('T', " ").trim_end_matches('Z').to_string() + " UTC"
}

#[cfg(target_os = "linux")]
fn format_human_local(secs: i64) -> String {
    use std::mem::MaybeUninit;
    let t: libc::time_t = secs as libc::time_t;
    let mut result = MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r writes into result; we own the buffer and never
    // share it across threads. Per POSIX, tzset is implied by localtime_r.
    let tm = unsafe {
        if libc::localtime_r(&t, result.as_mut_ptr()).is_null() {
            return format_human_utc(secs);
        }
        result.assume_init()
    };
    let year = tm.tm_year + 1900;
    let mon = tm.tm_mon + 1;
    let day = tm.tm_mday;
    let hh = tm.tm_hour;
    let mm = tm.tm_min;
    let ss = tm.tm_sec;
    let zone = unsafe {
        if tm.tm_zone.is_null() {
            None
        } else {
            std::ffi::CStr::from_ptr(tm.tm_zone)
                .to_str()
                .ok()
                .map(|s| s.to_string())
        }
    };
    // A bare offset like "-04" reads as orphaned next to the timestamp, so
    // prefix it with "UTC". Named zones ("CLT", "EST") are left alone.
    let label = match zone {
        Some(z) if !z.is_empty() => {
            if z.starts_with('+') || z.starts_with('-') {
                format!("UTC{z}")
            } else {
                z
            }
        }
        _ => {
            let off = tm.tm_gmtoff;
            let sign = if off >= 0 { '+' } else { '-' };
            let abs = off.unsigned_abs();
            let oh = abs / 3600;
            let om = (abs % 3600) / 60;
            format!("UTC{sign}{oh:02}:{om:02}")
        }
    };
    format!("{year:04}-{mon:02}-{day:02} {hh:02}:{mm:02}:{ss:02} {label}")
}

#[cfg(not(target_os = "linux"))]
fn format_human_local(secs: i64) -> String {
    format_human_utc(secs)
}

// Howard Hinnant's "days from civil" inverse — avoids pulling chrono just to
// format a timestamp. Format only, never parse.
fn format_iso_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let mut rem = secs.rem_euclid(86_400);
    let hh = (rem / 3600) as u32;
    rem %= 3600;
    let mm = (rem / 60) as u32;
    let ss = (rem % 60) as u32;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!(
        "{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z",
        year = year,
        m = m,
        d = d,
        hh = hh,
        mm = mm,
        ss = ss
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::config::ObsidianConfig;

    #[test]
    fn iso_known_epoch() {
        assert_eq!(format_iso_utc(1_709_251_200), "2024-03-01T00:00:00Z");
        assert_eq!(format_iso_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn human_known_epoch() {
        assert_eq!(format_human_utc(1_709_251_200), "2024-03-01 00:00:00 UTC");
        assert_eq!(format_human_utc(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn local_format_shape() {
        let s = format_human_local(1_709_251_200);
        let parts: Vec<&str> = s.split_whitespace().collect();
        assert_eq!(parts.len(), 3, "got {s:?}");
        assert_eq!(parts[0].len(), 10, "date YYYY-MM-DD: {:?}", parts[0]);
        assert_eq!(parts[1].len(), 8, "time HH:MM:SS: {:?}", parts[1]);
        assert!(!parts[2].is_empty(), "zone label: {:?}", parts[2]);
    }

    #[test]
    fn append_noop_when_path_unset() {
        let cfg = ObsidianConfig::defaults();
        let out = QuickCaptureWriter::append(&cfg, "hello", None, None).unwrap();
        assert!(out.is_none());
    }

    #[test]
    fn append_creates_dir_and_block() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("inbox").join("capture.md");
        let cfg = ObsidianConfig {
            quick_capture_path: Some(target.to_string_lossy().into_owned()),
            ..ObsidianConfig::defaults()
        };
        let out =
            QuickCaptureWriter::append(&cfg, "a thought", None, Some("/home/me/proj")).unwrap();
        assert_eq!(out.as_deref(), Some(target.as_path()));
        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.contains("a thought"));
        assert!(contents.contains("#nergal-inbox"));
        // Fresh inbox gets the one-time Open-in-Nergal deep link.
        assert!(contents.starts_with("# Inbox"));
        assert!(contents.contains("cluihud://open-workspace?path=%2Fhome%2Fme%2Fproj"));
    }

    #[test]
    fn append_custom_tag() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("capture.md");
        let cfg = ObsidianConfig {
            quick_capture_path: Some(target.to_string_lossy().into_owned()),
            ..ObsidianConfig::defaults()
        };
        QuickCaptureWriter::append(&cfg, "hi", Some("brain-dump"), None).unwrap();
        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.contains("#brain-dump"));
        assert!(!contents.contains("#nergal-inbox"));
    }

    #[test]
    fn session_log_writes_full_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("log").join("project.md");
        let cfg = ObsidianConfig {
            session_log_path: Some(target.to_string_lossy().into_owned()),
            ..ObsidianConfig::defaults()
        };
        SessionLogWriter::start_session(
            &cfg,
            "Auth refactor",
            "claude-code",
            Some("opus"),
            Some("/repo"),
        )
        .unwrap();
        SessionLogWriter::append_event(&cfg, "Edit src/auth.rs").unwrap();
        SessionLogWriter::end_session(&cfg, Some("opus"), &["/repo/src/auth.rs".to_string()], 2)
            .unwrap();
        let c = std::fs::read_to_string(&target).unwrap();
        assert!(c.contains("## Session \"Auth refactor\""));
        assert!(c.contains("- Agent: claude-code (opus)"));
        assert!(c.contains("- Model: opus"));
        assert!(c.contains("### Activity"));
        assert!(c.contains("· Edit src/auth.rs"));
        assert!(c.contains("### Session ended at"));
        assert!(c.contains("Files touched: 1 (auth.rs)"));
        assert!(c.contains("Tasks completed: 2"));
    }

    #[test]
    fn session_log_noop_when_path_unset() {
        let cfg = ObsidianConfig::defaults();
        SessionLogWriter::start_session(&cfg, "x", "claude-code", None, None).unwrap();
        SessionLogWriter::append_event(&cfg, "evt").unwrap();
        SessionLogWriter::end_session(&cfg, None, &[], 0).unwrap();
    }

    #[test]
    fn log_rotates_when_oversized() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Logs.md");
        std::fs::write(&target, "x".repeat(MAX_LOG_BYTES as usize + 1)).unwrap();
        let cfg = ObsidianConfig {
            session_log_path: Some(target.to_string_lossy().into_owned()),
            ..ObsidianConfig::defaults()
        };
        SessionLogWriter::start_session(&cfg, "S", "cc", None, None).unwrap();
        assert!(dir.path().join("Logs (archive).md").exists());
        let active = std::fs::read_to_string(&target).unwrap();
        assert!(active.len() < MAX_LOG_BYTES as usize);
        assert!(active.contains("## Session \"S\""));
    }

    #[test]
    fn append_appends_not_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("capture.md");
        let cfg = ObsidianConfig {
            quick_capture_path: Some(target.to_string_lossy().into_owned()),
            ..ObsidianConfig::defaults()
        };
        QuickCaptureWriter::append(&cfg, "first", None, None).unwrap();
        QuickCaptureWriter::append(&cfg, "second", None, None).unwrap();
        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.contains("first"));
        assert!(contents.contains("second"));
        let first_pos = contents.find("first").unwrap();
        let second_pos = contents.find("second").unwrap();
        assert!(first_pos < second_pos);
    }
}
