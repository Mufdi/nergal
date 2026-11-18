use super::conversation::{ResumeCandidates, pick_resume_id};
use std::path::{Path, PathBuf};

const A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const B: &str = "bbbbbbbb-0000-0000-0000-000000000002";
const C: &str = "cccccccc-0000-0000-0000-000000000003";

fn transcript(projects: &Path, project: &str, id: &str, continued_in: Option<&str>) {
    let dir = projects.join(project);
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = format!("{{\"type\":\"user\",\"sessionId\":\"{id}\"}}\n");
    if let Some(next) = continued_in {
        body.push_str(&format!(
            "{{\"type\":\"continued-in\",\"timestamp\":\"2026-11-18T02:59:11.918Z\",\"sessionId\":\"{id}\",\"continuedInSessionId\":\"{next}\"}}\n"
        ));
        body.push_str(&format!(
            "{{\"type\":\"last-prompt\",\"sessionId\":\"{id}\"}}\n"
        ));
    }
    std::fs::write(dir.join(format!("{id}.jsonl")), body).unwrap();
}

fn stored(id: &str) -> ResumeCandidates {
    ResumeCandidates {
        stored_id: Some(id.to_string()),
        transcript_path: None,
        claimed_by_older: false,
        is_oldest_in_workspace: false,
        repo_root: PathBuf::from("/repo"),
    }
}

#[test]
fn a_stored_conversation_continued_elsewhere_resumes_the_continuation() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("projects");
    transcript(&projects, "-repo--worktrees-nergal-x", A, Some(B));
    transcript(&projects, "-repo", B, None);

    assert_eq!(pick_resume_id(&stored(A), &[projects]), Some(B.to_string()));
}

#[test]
fn a_chain_of_continuations_resumes_the_last_link() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("projects");
    transcript(&projects, "-repo", A, Some(B));
    transcript(&projects, "-repo", B, Some(C));
    transcript(&projects, "-repo", C, None);

    assert_eq!(pick_resume_id(&stored(A), &[projects]), Some(C.to_string()));
}

#[test]
fn a_continuation_without_a_transcript_keeps_the_stored_conversation() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("projects");
    transcript(&projects, "-repo", A, Some(B));

    assert_eq!(pick_resume_id(&stored(A), &[projects]), Some(A.to_string()));
}

#[test]
fn a_continuation_cycle_terminates_on_an_existing_conversation() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("projects");
    transcript(&projects, "-repo", A, Some(B));
    transcript(&projects, "-repo", B, Some(A));

    let picked = pick_resume_id(&stored(A), &[projects]);

    assert!(
        picked.as_deref() == Some(A) || picked.as_deref() == Some(B),
        "{picked:?}"
    );
}

#[test]
fn a_continuation_id_that_is_not_a_conversation_id_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("projects");
    transcript(&projects, "-repo", A, Some("--fork-session"));

    assert_eq!(pick_resume_id(&stored(A), &[projects]), Some(A.to_string()));
}
