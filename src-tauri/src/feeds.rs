//! Provider status feed — polls the public Statuspage APIs of the model
//! providers behind the wrapped agents and surfaces active incidents.
//!
//! Only Claude (Anthropic) and OpenAI have dedicated pages worth watching:
//! OpenCode and Pi are model-agnostic, so they have no provider status of
//! their own. Both endpoints are Statuspage v2 (`/api/v2/status.json`),
//! verified 2026-06-06.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);

const PROVIDERS: &[(&str, &str, &str)] = &[
    (
        "claude",
        "https://status.claude.com/api/v2/status.json",
        "https://status.claude.com",
    ),
    (
        "openai",
        "https://status.openai.com/api/v2/status.json",
        "https://status.openai.com",
    ),
];

/// Wire form emitted to the frontend on every poll where something changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderStatus {
    pub provider: String,
    /// Statuspage indicator: "none" | "minor" | "major" | "critical"
    /// (or "unknown" when the page itself is unreachable — not surfaced as
    /// an incident; network-down would otherwise look like a provider
    /// outage).
    pub indicator: String,
    pub description: String,
    pub url: String,
}

#[derive(Deserialize)]
struct StatuspageResponse {
    status: StatuspageStatus,
}

#[derive(Deserialize)]
struct StatuspageStatus {
    indicator: String,
    description: String,
}

async fn fetch_status(
    client: &reqwest::Client,
    provider: &str,
    endpoint: &str,
    url: &str,
) -> Option<ProviderStatus> {
    let resp = client.get(endpoint).send().await.ok()?;
    let body: StatuspageResponse = resp.json().await.ok()?;
    Some(ProviderStatus {
        provider: provider.to_string(),
        indicator: body.status.indicator,
        description: body.status.description,
        url: url.to_string(),
    })
}

/// Long-lived background task. Spawn once from the Tauri setup block.
///
/// Emits `status:providers` with the full provider list on every poll — not
/// only on change — because Tauri events don't buffer: the first emit races
/// the webview's listener registration, and a webview reload would otherwise
/// stay empty until the next status *transition* (hours apart). The payload
/// is two tiny structs every 5 minutes; the frontend toasts only on
/// transitions it observes.
pub async fn run_status_feed(app: AppHandle) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("nergal-status-feed")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("status feed disabled: reqwest client build failed: {e}");
            return;
        }
    };

    let mut previous: Option<Vec<ProviderStatus>> = None;
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        let mut current = Vec::with_capacity(PROVIDERS.len());
        for (provider, endpoint, url) in PROVIDERS {
            // An unreachable page is reported as "unknown", never as an
            // incident — the frontend ignores it.
            let status = fetch_status(&client, provider, endpoint, url)
                .await
                .unwrap_or_else(|| ProviderStatus {
                    provider: provider.to_string(),
                    indicator: "unknown".to_string(),
                    description: String::new(),
                    url: url.to_string(),
                });
            current.push(status);
        }

        if previous.as_ref() != Some(&current)
            && current
                .iter()
                .any(|s| s.indicator != "none" && s.indicator != "unknown")
        {
            tracing::info!("provider status changed: {current:?}");
        }
        if let Err(e) = app.emit("status:providers", &current) {
            tracing::warn!("emit status:providers failed: {e}");
        }
        previous = Some(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuspage_response_parses_v2_shape() {
        let json = r#"{"page":{"id":"x","name":"Claude"},"status":{"indicator":"minor","description":"Partial outage"}}"#;
        let parsed: StatuspageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.status.indicator, "minor");
        assert_eq!(parsed.status.description, "Partial outage");
    }
}
