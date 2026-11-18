use crate::db::Database;
use std::path::{Path, PathBuf};

pub struct ResumeCandidates {
    pub stored_id: Option<String>,
    pub transcript_path: Option<PathBuf>,
    pub claimed_by_older: bool,
    pub is_oldest_in_workspace: bool,
    pub repo_root: PathBuf,
}

pub fn is_valid_conversation_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn file_stem(path: &Path) -> Option<String> {
    path.file_stem().map(|s| s.to_string_lossy().into_owned())
}

pub fn resume_candidates(db: &Database, session_id: &str) -> Option<ResumeCandidates> {
    let (repo_root, peers) = db.workspace_resume_peers(session_id).ok().flatten()?;
    let me = peers.iter().find(|p| p.id == session_id)?;
    let transcript_path = me.transcript_path.as_deref().map(PathBuf::from);
    let stem = transcript_path.as_deref().and_then(file_stem);

    let mut claimed_by_older = false;
    let mut is_oldest_in_workspace = true;
    for peer in peers.iter().filter(|p| p.created_at < me.created_at) {
        is_oldest_in_workspace = false;
        let same_transcript = transcript_path.is_some()
            && peer.transcript_path.as_deref().map(Path::new) == transcript_path.as_deref();
        let same_id = stem.is_some() && peer.internal_id == stem;
        if same_transcript || same_id {
            claimed_by_older = true;
        }
    }

    Some(ResumeCandidates {
        stored_id: me.internal_id.clone(),
        transcript_path,
        claimed_by_older,
        is_oldest_in_workspace,
        repo_root,
    })
}

fn find_transcript(id: &str, projects_dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = format!("{id}.jsonl");
    projects_dirs.iter().find_map(|dir| {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|project| project.path().join(&file))
            .find(|candidate| candidate.is_file())
    })
}

fn transcript_exists(id: &str, projects_dirs: &[PathBuf]) -> bool {
    find_transcript(id, projects_dirs).is_some()
}

const CONTINUATION_TAIL_BYTES: u64 = 64 * 1024;
const MAX_CONTINUATION_HOPS: usize = 10;

// Continuation markers are appended at the end; reading only the tail keeps this cheap on multi-MB transcripts.
fn continued_in(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(CONTINUATION_TAIL_BYTES)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    String::from_utf8_lossy(&tail)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some("continued-in"))
        .filter_map(|v| {
            v.get("continuedInSessionId")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
        })
        .next_back()
}

fn follow_continuations(start: String, projects_dirs: &[PathBuf]) -> String {
    let mut visited = vec![start.clone()];
    let mut current = start;
    for _ in 0..MAX_CONTINUATION_HOPS {
        let Some(next) = find_transcript(&current, projects_dirs)
            .and_then(|path| continued_in(&path))
            .filter(|id| is_valid_conversation_id(id) && !visited.contains(id))
            .filter(|id| transcript_exists(id, projects_dirs))
        else {
            break;
        };
        visited.push(next.clone());
        current = next;
    }
    current
}

pub fn pick_resume_id(c: &ResumeCandidates, projects_dirs: &[PathBuf]) -> Option<String> {
    if let Some(id) = &c.stored_id {
        return (is_valid_conversation_id(id) && transcript_exists(id, projects_dirs))
            .then(|| follow_continuations(id.clone(), projects_dirs));
    }

    let transcript = c.transcript_path.as_deref()?;
    let stem = file_stem(transcript)?;
    if !is_valid_conversation_id(&stem) || c.claimed_by_older {
        return None;
    }
    let in_repo_root_project = transcript
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.to_string_lossy() == project_dir_name(&c.repo_root));
    if in_repo_root_project && !c.is_oldest_in_workspace {
        return None;
    }
    transcript_exists(&stem, projects_dirs).then(|| follow_continuations(stem, projects_dirs))
}

pub fn projects_dirs(transcript_path: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    match config_dir.or_else(|| dirs::home_dir().map(|h| h.join(".claude"))) {
        Some(base) => dirs.push(base.join("projects")),
        None => tracing::warn!("no home directory: Claude Code projects dir unresolved"),
    }
    if let Some(extra) = transcript_path
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        && !dirs.contains(&extra)
    {
        dirs.push(extra);
    }
    dirs
}
