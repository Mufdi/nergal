use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::tasks::TaskStore;

/// Parse all TaskCreate/TaskUpdate events from a Claude Code transcript (.jsonl).
///
/// Each line is a JSON object with `message.content[]` array. We look for
/// entries where `type == "tool_use"` and `name` is "TaskCreate" or "TaskUpdate".
pub fn parse_tasks_from_transcript(path: &Path) -> TaskStore {
    let mut store = TaskStore::new();

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("failed to open transcript {}: {e}", path.display());
            return store;
        }
    };

    let reader = BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        for item in content {
            let Some(item_type) = item.get("type").and_then(|t| t.as_str()) else {
                continue;
            };
            if item_type != "tool_use" {
                continue;
            }

            let Some(name) = item.get("name").and_then(|n| n.as_str()) else {
                continue;
            };

            let Some(input) = item.get("input") else {
                continue;
            };

            match name {
                "TaskCreate" => {
                    store.apply_create(input);
                }
                "TaskUpdate" => {
                    store.apply_update(input);
                }
                _ => {}
            }
        }
    }

    store
}
