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

fn transcript_exists(id: &str, projects_dirs: &[PathBuf]) -> bool {
    let file = format!("{id}.jsonl");
    projects_dirs.iter().any(|dir| {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|project| project.path().join(&file).is_file())
            })
            .unwrap_or(false)
    })
}

pub fn pick_resume_id(c: &ResumeCandidates, projects_dirs: &[PathBuf]) -> Option<String> {
    if let Some(id) = &c.stored_id {
        return (is_valid_conversation_id(id) && transcript_exists(id, projects_dirs))
            .then(|| id.clone());
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
    transcript_exists(&stem, projects_dirs).then_some(stem)
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
