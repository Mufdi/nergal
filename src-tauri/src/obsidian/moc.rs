//! #11 per-session MOC snapshot + N1 reverse backlinks. Built by the detached
//! post-session runner from on-disk + DB state (no live atoms): a pure template
//! over (continuous-log block) + (DB cost/tasks/annotations) + (git diff stats).
//! N1 then writes a delimited backlink region into each note the MOC references.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::db::Database;
use crate::obsidian::channels::{iso_from_unix, iso_timestamp};
use crate::obsidian::config::ResolvedObsidianConfig;
use crate::tasks::TaskStatus;

const BACKLINK_START: &str = "<!-- nergal-backlinks-start -->";
const BACKLINK_END: &str = "<!-- nergal-backlinks-end -->";
const BACKLINK_CAP: usize = 50;

pub struct MocBuilder;

impl MocBuilder {
    /// Write `<moc_path>/<slug>-<YYYY-MM-DD>.md`. The date is the session's start
    /// day so re-runs overwrite (idempotent). None when moc_path is unset.
    pub fn build(
        session_id: &str,
        cfg: &ResolvedObsidianConfig,
        db: &Database,
    ) -> Result<Option<PathBuf>> {
        let Some(moc_dir) = cfg.moc_path.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let session = db
            .find_session(session_id)?
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;

        let cost_usd = db
            .get_cost(session_id)
            .ok()
            .flatten()
            .map(|c| c.total_usd)
            .unwrap_or(0.0);
        let tasks_done = db
            .get_visible_tasks(session_id)
            .unwrap_or_default()
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Completed))
            .count();
        let annotations = db.get_annotations(session_id).unwrap_or_default();

        let files = session
            .worktree_path
            .as_deref()
            .map(|wt| git_changed_files(wt, session.merge_target.as_deref()))
            .unwrap_or_default();

        let activity = cfg
            .session_log_path
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|p| extract_session_activity(Path::new(p), &session.name))
            .unwrap_or_default();

        let started = iso_from_unix(session.created_at as i64);
        let decisions: Vec<String> = annotations
            .iter()
            .map(|a| a.content.trim().replace('\n', " "))
            .filter(|s| !s.is_empty())
            .collect();

        let md = render_moc(
            &session.id,
            &session.agent_id,
            &session.name,
            &started,
            &iso_timestamp(),
            cost_usd,
            &files,
            tasks_done,
            &activity,
            &decisions,
        );

        let slug = {
            let s = crate::obsidian::bootstrap::slugify_for_vault(&session.name);
            if s.is_empty() {
                "session".to_string()
            } else {
                s
            }
        };
        let date = &started[..10.min(started.len())];
        let dir = Path::new(moc_dir);
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating moc dir {}", dir.display()))?;
        let out = dir.join(format!("{slug}-{date}.md"));
        atomic_write(&out, &md)?;
        Ok(Some(out))
    }
}

pub struct BacklinkUpdater;

impl BacklinkUpdater {
    /// For every `[[wikilink]]` in the MOC, append a backlink to the MOC into the
    /// target note's delimited region. No-op unless backlinks are enabled.
    pub fn propagate(moc_path: &Path, cfg: &ResolvedObsidianConfig) -> Result<()> {
        if !cfg.backlinks_enabled {
            return Ok(());
        }
        let Some(vault_root) = cfg.vault_root.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(());
        };
        let vault_root = Path::new(vault_root);
        let moc_content = std::fs::read_to_string(moc_path)?;
        let moc_slug = moc_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let entry = format!("- [[{moc_slug}]]");
        for target in parse_wikilinks(&moc_content) {
            if let Some(note) = resolve_vault_note(vault_root, &target) {
                let _ = update_backlink_region(&note, &entry);
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn render_moc(
    id: &str,
    agent: &str,
    name: &str,
    started: &str,
    ended: &str,
    cost_usd: f64,
    files: &[String],
    tasks_done: usize,
    activity: &str,
    decisions: &[String],
) -> String {
    let mut md = String::new();
    md.push_str("---\n");
    md.push_str(&format!("session_id: {id}\n"));
    md.push_str(&format!("agent: {agent}\n"));
    md.push_str(&format!("started_at: {started}\n"));
    md.push_str(&format!("ended_at: {ended}\n"));
    md.push_str(&format!("cost_usd: {cost_usd:.4}\n"));
    md.push_str(&format!("files_count: {}\n", files.len()));
    md.push_str(&format!("tasks_count: {tasks_done}\n"));
    md.push_str("---\n\n");
    md.push_str(&format!("# Session: {name}\n\n"));

    md.push_str("## Activity\n");
    if activity.trim().is_empty() {
        md.push_str("_No activity recorded._\n");
    } else {
        md.push_str(activity.trim_end());
        md.push('\n');
    }

    md.push_str("\n## Files touched\n");
    if files.is_empty() {
        md.push_str("_None._\n");
    } else {
        for f in files {
            md.push_str(&format!("- `{f}`\n"));
        }
    }

    md.push_str("\n## Decisions\n");
    if decisions.is_empty() {
        md.push_str("_None recorded._\n");
    } else {
        for d in decisions {
            md.push_str(&format!("- {d}\n"));
        }
    }
    md
}

/// `git diff --name-only <base>...HEAD` (or vs HEAD when no base). Best-effort:
/// any failure yields an empty list rather than aborting the MOC.
fn git_changed_files(worktree: &Path, base: Option<&str>) -> Vec<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(worktree).arg("diff").arg("--name-only");
    match base {
        Some(b) if !b.is_empty() => {
            cmd.arg(format!("{b}...HEAD"));
        }
        _ => {
            cmd.arg("HEAD");
        }
    }
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Pull the activity feed for `session_name` out of the continuous log: the
/// lines between `### Activity` and `### Session ended` under the most recent
/// matching `## Session "<name>"` header.
fn extract_session_activity(log_path: &Path, session_name: &str) -> Option<String> {
    let content = std::fs::read_to_string(log_path).ok()?;
    let header = format!("## Session \"{session_name}\" —");
    let start = content.rmatch_indices(&header).next().map(|(i, _)| i)?;
    let rest = &content[start..];
    // Block ends at the next session header or EOF.
    let block_end = rest[header.len()..]
        .find("\n## Session ")
        .map(|i| header.len() + i)
        .unwrap_or(rest.len());
    let block = &rest[..block_end];
    let activity = match block.find("### Activity") {
        Some(a) => {
            let after = &block[a + "### Activity".len()..];
            let end = after.find("### Session ended").unwrap_or(after.len());
            after[..end].trim().to_string()
        }
        None => block.trim().to_string(),
    };
    Some(activity)
}

/// Bare note names from `[[Note]]`, `[[Note|alias]]`, `[[Note#h]]`, `[[Note^b]]`.
/// Only the name segment matters for backlink resolution; deduped, order kept.
fn parse_wikilinks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        if let Some(close) = after.find("]]") {
            let inner = &after[..close];
            let name = inner.split(['|', '#', '^']).next().unwrap_or("").trim();
            if !name.is_empty() && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
            rest = &after[close + 2..];
        } else {
            break;
        }
    }
    out
}

fn resolve_vault_note(vault_root: &Path, target: &str) -> Option<PathBuf> {
    let direct = vault_root.join(format!("{target}.md"));
    if direct.is_file() {
        return Some(direct);
    }
    let want = format!("{target}.md");
    for entry in walkdir::WalkDir::new(vault_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy() == want {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}

fn build_region(entries: &[String]) -> String {
    let (recent, archived) = if entries.len() > BACKLINK_CAP {
        (&entries[..BACKLINK_CAP], &entries[BACKLINK_CAP..])
    } else {
        (entries, &[][..])
    };
    let mut region = String::new();
    region.push_str(BACKLINK_START);
    region.push_str("\n## Referenced in Nergal sessions\n");
    for e in recent {
        region.push_str(e);
        region.push('\n');
    }
    if !archived.is_empty() {
        region.push_str("<details><summary>Older</summary>\n\n");
        for e in archived {
            region.push_str(e);
            region.push('\n');
        }
        region.push_str("\n</details>\n");
    }
    region.push_str(BACKLINK_END);
    region
}

fn update_backlink_region(note: &Path, entry: &str) -> Result<()> {
    let content = std::fs::read_to_string(note).unwrap_or_default();
    let start = content.find(BACKLINK_START);
    let end = content.find(BACKLINK_END);
    let (head, existing, tail, had_region) = match (start, end) {
        (Some(s), Some(e)) if e > s => {
            let region = &content[s + BACKLINK_START.len()..e];
            let entries: Vec<String> = region
                .lines()
                .map(str::trim)
                .filter(|l| l.starts_with("- "))
                .map(String::from)
                .collect();
            (
                content[..s].to_string(),
                entries,
                content[e + BACKLINK_END.len()..].to_string(),
                true,
            )
        }
        _ => (String::new(), Vec::new(), String::new(), false),
    };
    if existing.iter().any(|x| x == entry) {
        return Ok(()); // idempotent
    }
    let mut entries = Vec::with_capacity(existing.len() + 1);
    entries.push(entry.to_string()); // recent-first
    entries.extend(existing);
    let region = build_region(&entries);
    let newc = if had_region {
        format!("{head}{region}{tail}")
    } else {
        format!("{}\n\n{region}\n", content.trim_end())
    };
    atomic_write(note, &newc)
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_activity_picks_latest_block_feed() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.md");
        std::fs::write(
            &log,
            "## Session \"Work\" — 2024-01-01T00:00:00Z\n- Agent: cc\n\n### Activity\n- t · old\n\n### Session ended at x\n\n## Session \"Work\" — 2024-02-02T00:00:00Z\n- Agent: cc\n\n### Activity\n- t · Edit a.rs\n- t · Read b.rs\n\n### Session ended at y\n",
        )
        .unwrap();
        let a = extract_session_activity(&log, "Work").unwrap();
        assert!(a.contains("Edit a.rs"));
        assert!(a.contains("Read b.rs"));
        assert!(!a.contains("old"), "should use the most recent block");
        assert!(!a.contains("Session ended"));
    }

    #[test]
    fn extract_activity_none_when_no_block() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.md");
        std::fs::write(&log, "## Session \"Other\" — x\n### Activity\n- t · x\n").unwrap();
        assert!(extract_session_activity(&log, "Missing").is_none());
    }

    #[test]
    fn wikilinks_parse_bare_names_dedup() {
        let links = parse_wikilinks(
            "see [[Auth]] and [[Auth|the auth note]] and [[DB#schema]] and [[N^b1]]",
        );
        assert_eq!(links, vec!["Auth", "DB", "N"]);
    }

    #[test]
    fn render_handles_empty_sections() {
        let md = render_moc(
            "id1",
            "cc",
            "My Sess",
            "2024-01-01T00:00:00Z",
            "2024-01-01T01:00:00Z",
            0.0,
            &[],
            0,
            "",
            &[],
        );
        assert!(md.contains("session_id: id1"));
        assert!(md.contains("# Session: My Sess"));
        assert!(md.contains("_No activity recorded._"));
        assert!(md.contains("_None._"));
        assert!(md.contains("_None recorded._"));
    }

    #[test]
    fn backlink_new_note_appends_region() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Auth.md");
        std::fs::write(&note, "# Auth\n\nbody\n").unwrap();
        update_backlink_region(&note, "- [[sess-2024-01-01]]").unwrap();
        let c = std::fs::read_to_string(&note).unwrap();
        assert!(c.contains(BACKLINK_START));
        assert!(c.contains("## Referenced in Nergal sessions"));
        assert!(c.contains("- [[sess-2024-01-01]]"));
        assert!(c.contains(BACKLINK_END));
        assert!(c.starts_with("# Auth"));
    }

    #[test]
    fn backlink_existing_region_prepends_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Auth.md");
        std::fs::write(&note, "# Auth\n").unwrap();
        update_backlink_region(&note, "- [[first]]").unwrap();
        update_backlink_region(&note, "- [[second]]").unwrap();
        update_backlink_region(&note, "- [[second]]").unwrap(); // dup → no-op
        let c = std::fs::read_to_string(&note).unwrap();
        assert_eq!(c.matches("- [[second]]").count(), 1);
        // recent-first: second appears before first
        assert!(c.find("- [[second]]").unwrap() < c.find("- [[first]]").unwrap());
        // single region (not duplicated)
        assert_eq!(c.matches(BACKLINK_START).count(), 1);
    }

    #[test]
    fn backlink_caps_at_50_into_details() {
        let entries: Vec<String> = (0..55).map(|i| format!("- [[s{i}]]")).collect();
        let region = build_region(&entries);
        assert!(region.contains("<details>"));
        // newest 50 outside details, oldest 5 inside.
        let details_at = region.find("<details>").unwrap();
        assert!(region[..details_at].matches("- [[s").count() == BACKLINK_CAP);
    }

    #[test]
    fn resolve_note_direct_and_recursive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Top.md"), "x").unwrap();
        let nested = dir.path().join("sub").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("Buried.md"), "x").unwrap();
        assert!(resolve_vault_note(dir.path(), "Top").is_some());
        assert!(resolve_vault_note(dir.path(), "Buried").is_some());
        assert!(resolve_vault_note(dir.path(), "Ghost").is_none());
    }
}
