//! Provider-agnostic API-key summarizer: POSTs to an OpenAI-compatible
//! `/chat/completions` endpoint (OpenAI, OpenRouter, a local server, etc.). NOT
//! Anthropic-locked. The key is read from the OS keyring by the caller and
//! passed in — it never lands in config or logs.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde_json::json;

use super::{INSTRUCTION, Summarizer, Summary};

pub struct ApiKeyBackend {
    base_url: String,
    model: String,
    key: String,
}

impl ApiKeyBackend {
    pub fn new(base_url: String, model: String, key: String) -> Self {
        Self {
            base_url,
            model,
            key,
        }
    }

    /// `{base}/chat/completions`, tolerating a trailing slash on the base URL.
    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl Summarizer for ApiKeyBackend {
    async fn summarize(&self, transcript: &str) -> Result<Summary> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": INSTRUCTION },
                { "role": "user", "content": transcript },
            ],
            "temperature": 0.2,
        });

        let resp = reqwest::Client::new()
            .post(self.endpoint())
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .context("calling summary API")?;

        let status = resp.status();
        if !status.is_success() {
            // Body may echo the request; never surface the key. reqwest's error
            // text does not contain the auth header, so the body is safe to show.
            let text = resp.text().await.unwrap_or_default();
            bail!("summary API returned {status}: {}", text.trim());
        }

        let parsed: serde_json::Value =
            resp.json().await.context("parsing summary API response")?;
        let text = parsed
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let Some(text) = text else {
            bail!("summary API response had no message content");
        };
        let token_cost = parsed
            .pointer("/usage/total_tokens")
            .and_then(|v| v.as_i64());
        Ok(Summary {
            text,
            model: Some(self.model.clone()),
            token_cost,
        })
    }
}
