//! Agent-CLI summarizer: invokes a headless agent (`<cmd> -p <prompt>`) on the
//! user's existing subscription — no API key. Verified: `claude -p "<prompt>"`
//! authenticates via the Claude Max plan with no key.
//!
//! The transcript + instruction are passed together as the single `-p`
//! argument (the verified invocation path), not piped on stdin: 48KB fits well
//! under ARG_MAX, and it avoids any ambiguity about how the CLI combines a
//! piped stdin with the prompt flag.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use tokio::process::Command;

use super::{INSTRUCTION, Summarizer, Summary};

/// Hard cap so a hung agent never strands the in-flight guard.
const TIMEOUT: Duration = Duration::from_secs(120);

pub struct AgentCliBackend {
    binary: String,
    /// Flags placed before the prompt argument (e.g. `["-p"]` for Claude Code).
    args: Vec<String>,
}

impl AgentCliBackend {
    pub fn new(binary: String, args: Vec<String>) -> Self {
        Self { binary, args }
    }
}

#[async_trait]
impl Summarizer for AgentCliBackend {
    async fn summarize(&self, transcript: &str) -> Result<Summary> {
        let prompt = format!("{INSTRUCTION}\n\n---\nSESSION TRANSCRIPT:\n{transcript}");
        tracing::info!(binary = %self.binary, "running agent-CLI summarizer");

        let child = Command::new(&self.binary)
            .args(&self.args)
            .arg(&prompt)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "spawning summarizer agent `{}` (is it on PATH?)",
                    self.binary
                )
            })?;

        let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(res) => res.context("waiting on summarizer agent")?,
            Err(_) => bail!("summarizer agent timed out after {}s", TIMEOUT.as_secs()),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "summarizer agent exited {}: {}",
                output.status,
                stderr.trim()
            );
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            bail!("summarizer agent produced no output");
        }
        Ok(Summary {
            text,
            // The binary stands in for the model: the headless CLI reports no
            // machine-readable model id or usage.
            model: Some(self.binary.clone()),
            token_cost: None,
        })
    }
}
