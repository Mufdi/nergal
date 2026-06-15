//! Opt-in AI session summaries (phase 6).
//!
//! Off by default. Two mutually-exclusive backends, chosen via the single
//! [`SummaryBackend`](crate::config::SummaryBackend) config enum:
//! - `AgentCli` — headless agent on the user's subscription, no API key.
//! - `ApiKey`   — provider-agnostic OpenAI-compatible HTTP, key in the keyring.
//!
//! The read path never invokes a backend; generation runs detached on `Stop`
//! and writes the `session_summaries` table the descriptor reads from.

mod agent_cli;
mod api_key;
pub mod runner;
mod secret;

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;

pub use secret::{clear_api_key, has_api_key, load_api_key, store_api_key};

use crate::agents::HeadlessPrintCommand;
use crate::config::{Config, SummaryBackend, SummaryConfig};
use agent_cli::AgentCliBackend;
use api_key::ApiKeyBackend;

/// Instruction handed to every backend. The transcript rides as the user/stdin
/// payload; this keeps the model focused on a terse, factual recap.
const INSTRUCTION: &str = "You are summarizing a coding-agent session transcript. \
In 2-3 sentences, state concretely what the session accomplished: the task, the \
files or components changed, and the outcome. No preamble, no bullet points, no \
markdown headers — just the summary prose.";

/// Max transcript bytes fed to a backend (~12k tokens at ~4 bytes/token). The
/// tail is kept, since the most recent turns carry the session's conclusion.
const TRANSCRIPT_BUDGET_BYTES: usize = 48_000;

/// A generated summary plus optional usage accounting.
#[derive(Debug, Clone)]
pub struct Summary {
    pub text: String,
    pub model: Option<String>,
    /// Total tokens billed when the backend reports usage; `None` for the
    /// headless agent CLI, which surfaces no machine-readable usage.
    pub token_cost: Option<i64>,
}

#[async_trait]
trait Summarizer {
    async fn summarize(&self, transcript: &str) -> Result<Summary>;
}

/// Read + truncate the transcript and run the selected backend. The caller is
/// responsible for having resolved `backend` via
/// [`Config::effective_summary_backend`](crate::config::Config::effective_summary_backend);
/// `Off` is rejected here as a guard.
pub async fn summarize_transcript(
    backend: SummaryBackend,
    cfg: &SummaryConfig,
    agent_cmd: Option<HeadlessPrintCommand>,
    transcript_path: &Path,
) -> Result<Summary> {
    let transcript = read_tail(transcript_path, TRANSCRIPT_BUDGET_BYTES)?;
    if transcript.trim().is_empty() {
        bail!("transcript is empty");
    }
    match backend {
        SummaryBackend::Off => bail!("summaries are disabled for this session"),
        SummaryBackend::AgentCli => {
            // Explicit per-summary override wins; otherwise the caller passes
            // the default agent's verified headless command.
            let cmd = match cfg.agent_command.as_deref().map(str::trim) {
                Some(bin) if !bin.is_empty() => HeadlessPrintCommand {
                    binary: bin.to_string(),
                    args: vec!["-p".to_string()],
                },
                _ => agent_cmd.ok_or_else(|| {
                    anyhow!(
                        "the default agent has no verified headless summary mode — \
                         set a summary command, switch the default agent to Claude Code, \
                         or use API-key mode"
                    )
                })?,
            };
            AgentCliBackend::new(cmd.binary, cmd.args)
                .summarize(&transcript)
                .await
        }
        SummaryBackend::ApiKey => {
            // Validate cheap config before the keyring lookup so missing
            // endpoint/model surface a clear error without a secret-service hit.
            let base = cfg
                .api_base_url
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("API-key mode enabled but no base URL configured"))?;
            let model = cfg
                .api_model
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("API-key mode enabled but no model configured"))?;
            let key =
                load_api_key()?.ok_or_else(|| anyhow!("API-key mode enabled but no key stored"))?;
            ApiKeyBackend::new(base, model, key)
                .summarize(&transcript)
                .await
        }
    }
}

/// Everything the Settings UI needs to render the two summary switches. The
/// API key itself is never returned — only whether one is stored.
#[derive(serde::Serialize)]
pub struct SummarySettings {
    pub backend: SummaryBackend,
    pub agent_command: Option<String>,
    pub api_base_url: Option<String>,
    pub api_model: Option<String>,
    pub has_api_key: bool,
    pub disabled_projects: Vec<String>,
}

#[tauri::command]
pub fn summary_get_settings() -> Result<SummarySettings, String> {
    let cfg = Config::load().summary;
    Ok(SummarySettings {
        backend: cfg.backend,
        agent_command: cfg.agent_command,
        api_base_url: cfg.api_base_url,
        api_model: cfg.api_model,
        has_api_key: has_api_key().unwrap_or(false),
        disabled_projects: cfg.disabled_projects,
    })
}

/// Persist the summary backend choice + its fields. Mutual exclusivity is
/// structural: `backend` is a single enum value, so there is no way to enable
/// both modes at once. The API key is set separately via [`set_summary_api_key`].
#[tauri::command]
pub fn summary_set_settings(
    backend: SummaryBackend,
    agent_command: Option<String>,
    api_base_url: Option<String>,
    api_model: Option<String>,
) -> Result<(), String> {
    let mut config = Config::load();
    config.summary.backend = backend;
    config.summary.agent_command = agent_command;
    config.summary.api_base_url = api_base_url;
    config.summary.api_model = api_model;
    config.save().map_err(|e| format!("{e:#}"))
}

/// Per-project opt-out: add/remove a project from `disabled_projects`.
#[tauri::command]
pub fn summary_set_project_disabled(project_path: String, disabled: bool) -> Result<(), String> {
    let mut config = Config::load();
    let key = Config::canonicalize_project_path(std::path::Path::new(&project_path));
    let list = &mut config.summary.disabled_projects;
    let present = list.iter().any(|p| p == &key);
    if disabled && !present {
        list.push(key);
    } else if !disabled && present {
        list.retain(|p| p != &key);
    }
    config.save().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn set_summary_api_key(key: String) -> Result<(), String> {
    store_api_key(&key).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn clear_summary_api_key() -> Result<(), String> {
    clear_api_key().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn has_summary_api_key() -> Result<bool, String> {
    has_api_key().map_err(|e| format!("{e:#}"))
}

/// Keep the last `budget` bytes of the file, aligned to a line boundary so a
/// backend never sees a half-line of JSONL. Returns the whole file when smaller.
fn read_tail(path: &Path, budget: usize) -> Result<String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("reading transcript {}: {e}", path.display()))?;
    if raw.len() <= budget {
        return Ok(raw);
    }
    let start = raw.len() - budget;
    // Advance to the next newline so the first retained line is whole.
    let aligned = raw[start..]
        .find('\n')
        .map(|nl| start + nl + 1)
        .unwrap_or(start);
    Ok(raw[aligned..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn off_backend_never_reads_or_invokes() {
        // Off is a guard error even with a valid transcript: the runner gates on
        // it earlier, but the entrypoint must never invoke a backend when Off.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "some transcript\n").unwrap();
        let cfg = SummaryConfig::default();
        let err = summarize_transcript(SummaryBackend::Off, &cfg, None, &p)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn api_key_mode_without_config_fails_fast() {
        // No base URL / model / key configured → a clear error, no panic, no
        // network call attempted past the missing-config guard.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "transcript body\n").unwrap();
        let cfg = SummaryConfig {
            backend: SummaryBackend::ApiKey,
            ..SummaryConfig::default()
        };
        let err = summarize_transcript(SummaryBackend::ApiKey, &cfg, None, &p)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("API-key mode") || msg.contains("key"));
    }

    #[test]
    fn read_tail_returns_whole_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "line1\nline2\n").unwrap();
        assert_eq!(read_tail(&p, 1000).unwrap(), "line1\nline2\n");
    }

    #[test]
    fn read_tail_keeps_whole_last_lines_under_budget() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        let body = (0..1000)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&p, &body).unwrap();
        let tail = read_tail(&p, 200).unwrap();
        assert!(tail.len() <= 200);
        // No leading partial line.
        assert!(tail.starts_with("line-"));
        // The very last line survives.
        assert!(tail.contains("line-999"));
    }
}
