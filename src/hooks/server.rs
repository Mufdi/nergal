use std::path::Path;

use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use super::events::HookEvent;

pub async fn start_hook_server(socket_path: &Path, tx: mpsc::Sender<HookEvent>) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!("hook server listening on {}", socket_path.display());

    loop {
        let (stream, _addr) = listener.accept().await?;
        let tx = tx.clone();

        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stream);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<HookEvent>(&line) {
                    Ok(event) => {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("failed to parse hook event: {e}");
                    }
                }
            }
        });
    }
}
