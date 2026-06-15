//! Agent-CLI summarizer: invokes a headless agent (`<cmd> -p <prompt>`) on the
//! user's existing subscription — no API key. Verified: `claude -p` authenticates
//! via the Claude Max plan with no key. The transcript is piped on stdin so it
//! never hits ARG_MAX; the instruction rides the `-p` flag.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{INSTRUCTION, Summarizer, Summary};

pub struct AgentCliBackend {
    command: String,
}

impl AgentCliBackend {
    pub fn new(command: String) -> Self {
        Self { command }
    }
}

#[async_trait]
impl Summarizer for AgentCliBackend {
    async fn summarize(&self, transcript: &str) -> Result<Summary> {
        let mut child = Command::new(&self.command)
            .arg("-p")
            .arg(INSTRUCTION)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning summarizer agent `{}`", self.command))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(transcript.as_bytes())
                .await
                .context("writing transcript to summarizer stdin")?;
            stdin.shutdown().await.ok();
        }

        let output = child
            .wait_with_output()
            .await
            .context("waiting on summarizer agent")?;
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
            // The headless CLI prints the model in stdout only as prose; we don't
            // parse it, and it reports no machine-readable usage.
            model: Some(self.command.clone()),
            token_cost: None,
        })
    }
}
