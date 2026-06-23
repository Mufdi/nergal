//! Agent-CLI summarizer: invokes a headless agent on the user's existing
//! subscription — no API key. Verified key-free this session for all four
//! adapters (claude `-p`, pi `-p`, codex `exec`, opencode `run`).
//!
//! Each agent recovers its answer differently
//! ([`HeadlessOutput`](crate::agents::HeadlessOutput)): clean stdout (claude,
//! pi), a `--output-last-message` file (codex, whose stdout carries a banner),
//! or `--format json` JSONL (opencode). The transcript + instruction ride as
//! the single final prompt argument (the verified path), not stdin.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use tokio::process::Command;

use super::{Summarizer, Summary};
use crate::agents::{HeadlessOutput, HeadlessPrintCommand};

/// Hard cap so a hung agent never strands the in-flight guard.
const TIMEOUT: Duration = Duration::from_secs(120);

pub struct AgentCliBackend {
    cmd: HeadlessPrintCommand,
}

impl AgentCliBackend {
    pub fn new(cmd: HeadlessPrintCommand) -> Self {
        Self { cmd }
    }
}

/// Unique temp path for the codex `--output-last-message` sink. pid + counter
/// avoids collisions when two sessions summarize concurrently.
fn unique_message_path() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("nergal-summary-{}-{n}.txt", std::process::id()))
}

#[async_trait]
impl Summarizer for AgentCliBackend {
    async fn summarize(&self, transcript: &str) -> Result<Summary> {
        let prompt = format!(
            "{}\n\n---\nSESSION TRANSCRIPT:\n{transcript}",
            super::INSTRUCTION
        );
        tracing::info!(binary = %self.cmd.binary, "running agent-CLI summarizer");

        // For LastMessageFile, append `<flag> <tmp_path>` before the prompt.
        let mut args = self.cmd.args.clone();
        let msg_file = match &self.cmd.output {
            HeadlessOutput::LastMessageFile { flag } => {
                let path = unique_message_path();
                args.push(flag.clone());
                args.push(path.to_string_lossy().into_owned());
                Some(path)
            }
            _ => None,
        };

        let child = Command::new(&self.cmd.binary)
            .args(&args)
            .arg(&prompt)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "spawning summarizer agent `{}` (is it on PATH?)",
                    self.cmd.binary
                )
            })?;

        let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(res) => res.context("waiting on summarizer agent")?,
            Err(_) => {
                if let Some(p) = &msg_file {
                    let _ = std::fs::remove_file(p);
                }
                bail!("summarizer agent timed out after {}s", TIMEOUT.as_secs());
            }
        };
        if !output.status.success() {
            if let Some(p) = &msg_file {
                let _ = std::fs::remove_file(p);
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "summarizer agent exited {}: {}",
                output.status,
                stderr.trim()
            );
        }

        let (text, token_cost) = match &self.cmd.output {
            HeadlessOutput::Stdout => (
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
                None,
            ),
            HeadlessOutput::LastMessageFile { .. } => {
                let path = msg_file.expect("LastMessageFile sets msg_file");
                let body = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading summarizer output {}", path.display()))?;
                let _ = std::fs::remove_file(&path);
                (body.trim().to_string(), None)
            }
            HeadlessOutput::OpencodeJsonl => {
                parse_opencode_jsonl(&String::from_utf8_lossy(&output.stdout))
            }
        };

        if text.is_empty() {
            bail!("summarizer agent produced no output");
        }
        Ok(Summary {
            text,
            // The binary stands in for the model: the headless CLI reports no
            // machine-readable model id.
            model: Some(self.cmd.binary.clone()),
            token_cost,
        })
    }
}

/// Concatenate the `part.text` of every `{"type":"text"}` event and sum
/// `part.tokens.total` across `step_finish` events. Malformed lines are skipped.
fn parse_opencode_jsonl(stdout: &str) -> (String, Option<i64>) {
    let mut text = String::new();
    let mut tokens: i64 = 0;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = v.pointer("/part/text").and_then(|t| t.as_str()) {
                    text.push_str(t);
                }
            }
            Some("step_finish") => {
                if let Some(n) = v.pointer("/part/tokens/total").and_then(|n| n.as_i64()) {
                    tokens += n;
                }
            }
            _ => {}
        }
    }
    let token_cost = (tokens > 0).then_some(tokens);
    (text.trim().to_string(), token_cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opencode_text_and_tokens() {
        let jsonl = r#"{"type":"step_start","part":{}}
{"type":"text","part":{"type":"text","text":"Refactored "}}
{"type":"text","part":{"type":"text","text":"the auth flow."}}
{"type":"step_finish","part":{"tokens":{"total":1234}}}"#;
        let (text, cost) = parse_opencode_jsonl(jsonl);
        assert_eq!(text, "Refactored the auth flow.");
        assert_eq!(cost, Some(1234));
    }

    #[test]
    fn opencode_skips_malformed_lines_and_zero_tokens() {
        let jsonl = "not json\n{\"type\":\"text\",\"part\":{\"text\":\"hi\"}}\n";
        let (text, cost) = parse_opencode_jsonl(jsonl);
        assert_eq!(text, "hi");
        assert_eq!(cost, None);
    }
}
