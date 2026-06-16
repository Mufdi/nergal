//! Pull-based, debounced summary runner (Revision 1).
//!
//! Generation is triggered lazily by the read path (`get_session`), never on
//! `Stop` — `Stop` only writes the cheap `session_transcripts` marker. The read
//! path does no FS/lock work: the whole prelude (config load, backend
//! resolution, dirty re-check) runs inside the detached task, so directory
//! reads never block. Three guards bound cost under caller-driven reads:
//! a per-session debounce window, a per-session single-flight set, and a
//! process-wide concurrency semaphore.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dashmap::{DashMap, DashSet};
use tokio::sync::Semaphore;

use crate::agents::state::AgentRuntimeState;
use crate::config::{Config, SummaryBackend};
use crate::db::SharedDb;

/// Minimum interval between summary generations for one session.
const DEBOUNCE: Duration = Duration::from_secs(60);

/// Max concurrent summary generations across all sessions — caps the cost a
/// `get_session` loop over many dirty sessions can amplify.
const MAX_CONCURRENT: usize = 2;

fn last_run() -> &'static DashMap<String, Instant> {
    static M: OnceLock<DashMap<String, Instant>> = OnceLock::new();
    M.get_or_init(DashMap::new)
}

fn in_flight() -> &'static DashSet<String> {
    static M: OnceLock<DashSet<String>> = OnceLock::new();
    M.get_or_init(DashSet::new)
}

fn gen_semaphore() -> &'static Semaphore {
    static S: OnceLock<Semaphore> = OnceLock::new();
    S.get_or_init(|| Semaphore::new(MAX_CONCURRENT))
}

/// Pull entrypoint: called from the read path when `get_session` resolves a
/// dirty session. Non-blocking. The caller passes the project repo path plus the
/// marker's transcript path and `last_stop_at` (all already in hand), so no DB
/// lock or FS read happens before the detached spawn.
///
/// `consumed_last_stop_at` is stamped into `summary.updated_at` on success, so a
/// `Stop` landing mid-generation keeps the session dirty (`last_stop_at >
/// updated_at`) and its final turn is regenerated on the next read.
///
/// Gate ordering (fixed): (1) debounce read → bail without spawning; (2)
/// single-flight `insert` (sole correctness gate) → bail if present; (3) arm the
/// debounce at spawn time; (4) spawn detached; (5) prelude + dirty re-check
/// inside the task.
pub fn maybe_spawn(
    db: &SharedDb,
    agents: &AgentRuntimeState,
    session_id: &str,
    project: &Path,
    transcript_path: &str,
    consumed_last_stop_at: u64,
) {
    // (1) Debounce: cheap DashMap read; bail without spawning if inside window.
    if let Some(prev) = last_run().get(session_id)
        && prev.elapsed() < DEBOUNCE
    {
        return;
    }
    // (2) Single-flight: `insert` returns false when already present → another
    //     run owns this session. This is the sole correctness gate.
    if !in_flight().insert(session_id.to_string()) {
        return;
    }
    // (3) Arm the debounce at spawn time, on EVERY spawn (success, failure, or
    //     dropped-in-prelude), so a perpetually-failing or summaries-disabled
    //     session is rate-capped rather than retried on every read.
    last_run().insert(session_id.to_string(), Instant::now());

    // (4) Spawn detached; the FS/lock prelude lives inside.
    let db = db.clone();
    let agents = agents.clone();
    let sid = session_id.to_string();
    let project = project.to_path_buf();
    let transcript = PathBuf::from(transcript_path);
    tauri::async_runtime::spawn(async move {
        run_generation(
            db,
            agents,
            &sid,
            &project,
            &transcript,
            consumed_last_stop_at,
        )
        .await;
        in_flight().remove(&sid);
    });
}

async fn run_generation(
    db: SharedDb,
    agents: AgentRuntimeState,
    sid: &str,
    project: &Path,
    transcript: &Path,
    consumed_last_stop_at: u64,
) {
    // (5) Prelude inside the task — none of this touches the read path.
    let cfg = Config::load();
    let backend = cfg.effective_summary_backend(project);
    if backend == SummaryBackend::Off {
        return;
    }
    // Dirty re-check under the single-flight guard: if a prior run already
    // covered this activity (its `updated_at` is at least our consumed
    // `last_stop_at`), skip — handles the race where generation completed
    // between the read's dirty test and this task starting.
    {
        let Ok(guard) = db.lock() else {
            return;
        };
        if let Ok(Some(existing)) = guard.get_session_summary(sid)
            && consumed_last_stop_at <= existing.updated_at
        {
            return;
        }
    }
    // Resolve the default agent's headless command (used only by AgentCli).
    let default_agent = cfg.default_agent.as_deref().unwrap_or("claude-code");
    let agent_cmd = agents.headless_print_command(default_agent);

    tracing::info!(session_id = %sid, ?backend, "generating session summary (pull)");

    // Cap concurrent generations process-wide; held only around the LLM call.
    // `acquire` only errors if the semaphore is closed (shutdown) — bail rather
    // than proceed past the cap if a future close path is ever added.
    let Ok(_permit) = gen_semaphore().acquire().await else {
        return;
    };

    let result =
        crate::mcp::summary::summarize_transcript(backend, &cfg.summary, agent_cmd, transcript)
            .await;
    match result {
        Ok(s) => {
            if let Ok(guard) = db.lock()
                && let Err(e) = guard.set_session_summary(
                    sid,
                    &s.text,
                    s.model.as_deref(),
                    s.token_cost,
                    consumed_last_stop_at,
                )
            {
                // A write-after-delete (the session was removed mid-generation)
                // surfaces here as an FK error — intended, logged, no crash.
                tracing::warn!(session_id = %sid, "persisting session summary failed: {e:#}");
            }
            tracing::info!(session_id = %sid, "session summary stored");
        }
        Err(e) => {
            // Best-effort: log and move on; a later read retries (debounced).
            tracing::warn!(session_id = %sid, "session summary generation failed: {e:#}");
        }
    }
}
