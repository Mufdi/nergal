use tauri::State;

use super::shared::resolve_session_cwd;
use crate::db::SharedDb;
use crate::hooks::state::HookState;
use crate::platform_spawn::NoWindow;

#[tauri::command]
pub fn get_conflicted_files(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::conflicted_files(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_file_conflict_versions(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
) -> Result<crate::worktree::ConflictVersions, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::file_conflict_versions(&cwd, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_conflict_resolution(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    merged: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let abs = crate::fs_guard::resolve_within_base(&cwd, &path)?;
    std::fs::write(&abs, merged).map_err(|e| format!("failed to write: {e}"))?;
    crate::worktree::stage_file(&cwd, &path).map_err(|e| e.to_string())?;
    crate::worktree::conflicted_files(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn build_conflict_prompt(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    ours: String,
    theirs: String,
    original_merged: String,
    intent: Option<String>,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = crate::worktree::current_branch(&cwd).unwrap_or_else(|_| "HEAD".into());

    let status_out = std::process::Command::new("git")
        .no_window()
        .args(["status", "--short"])
        .current_dir(&cwd)
        .output();
    let status = status_out
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let intent_section = intent
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("\n\nMy intent for this resolution:\n{s}\n"))
        .unwrap_or_default();

    let prompt = format!(
        "Resolve the merge conflict in `{path}` (worktree: `{cwd}`, branch: `{branch}`).\n\n\
         We ran `git merge --no-ff --no-commit <target>` which left MERGE_HEAD pending.\n\n\
         `git status --short`:\n```\n{status}```\n\n\
         --- ours (HEAD version) ---\n```\n{ours}\n```\n\n\
         --- theirs (incoming version) ---\n```\n{theirs}\n```\n\n\
         --- original working copy with conflict markers ---\n```\n{original_merged}\n```\
         {intent_section}\n\n\
         Steps:\n\
         1. Decide on the correct resolution, honoring my intent if stated.\n\
         2. Write the resolved contents back to `{path}` via the Edit tool.\n\
         3. Stage it with `git add {path}`.\n\
         4. If no other conflicts remain, finish the merge with `git commit --no-edit`.\n",
        cwd = cwd.display(),
    );
    Ok(prompt)
}

#[tauri::command]
pub fn enqueue_conflict_context(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    ours: String,
    theirs: String,
    merged: String,
    instruction: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = crate::worktree::current_branch(&cwd).unwrap_or_else(|_| "HEAD".into());

    let status_out = std::process::Command::new("git")
        .no_window()
        .args(["status", "--short"])
        .current_dir(&cwd)
        .output();
    let status = status_out
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let feedback = format!(
        "You are resolving a merge conflict in `{path}` inside the worktree at `{cwd}`.\n\
         Current branch: `{branch}`. We ran `git merge --no-ff --no-commit <target>` which left MERGE_HEAD pending.\n\
         {instruction}\n\n\
         `git status --short`:\n```\n{status}```\n\n\
         --- ours (HEAD version) ---\n```\n{ours}\n```\n\n\
         --- theirs (incoming version) ---\n```\n{theirs}\n```\n\n\
         --- current working copy with conflict markers ---\n```\n{merged}\n```\n\n\
         Next steps:\n\
         1. Decide on the correct resolution.\n\
         2. Write the resolved contents back to `{path}` via the Edit tool.\n\
         3. Stage it with `git add {path}`.\n\
         4. If no other conflicts remain, finish the merge with `git commit --no-edit`.\n",
        cwd = cwd.display(),
    );
    HookState::set_pending_annotations(feedback).map_err(|e| e.to_string())?;

    // Return a short seed prompt the frontend will write to the terminal.
    Ok(format!("Resolve the merge conflict in {path}"))
}
