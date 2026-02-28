use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// IPC state file for communication between the GUI app and hook CLI subcommands.
/// Lives at `~/.claude/cluihud-state.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HookState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_plan_edit: Option<PathBuf>,
}

impl HookState {
    fn state_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("home directory must exist")?;
        Ok(home.join(".claude").join("cluihud-state.json"))
    }

    /// Reads the state file. Returns default if file doesn't exist.
    pub fn read() -> Result<Self> {
        let path = Self::state_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading state: {}", path.display()))?;
        let state: Self =
            serde_json::from_str(&contents).with_context(|| "parsing cluihud-state.json")?;
        Ok(state)
    }

    /// Writes the state to disk.
    pub fn write(&self) -> Result<()> {
        let path = Self::state_path()?;
        let json = serde_json::to_string(self).context("serializing state")?;
        std::fs::write(&path, json)
            .with_context(|| format!("writing state: {}", path.display()))?;
        Ok(())
    }

    /// Reads the pending edit path, clears it from disk, and returns it.
    pub fn take_pending_edit() -> Result<Option<PathBuf>> {
        let mut state = Self::read()?;
        let pending = state.pending_plan_edit.take();
        if pending.is_some() {
            state.write()?;
        }
        Ok(pending)
    }
}
