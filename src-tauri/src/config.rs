use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Application configuration with persistence support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub claude_binary: String,
    pub plans_directory: PathBuf,
    pub transcripts_directory: PathBuf,
    pub hook_socket_path: PathBuf,
    pub default_shell: String,
    pub theme_mode: String,
    pub preferred_editor: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().expect("home directory must exist");
        let claude_dir = home.join(".claude");
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        Self {
            claude_binary: "claude".into(),
            plans_directory: claude_dir.join("plans"),
            transcripts_directory: claude_dir.join("projects"),
            hook_socket_path: std::env::temp_dir().join("cluihud.sock"),
            default_shell: shell,
            theme_mode: "dark".into(),
            preferred_editor: String::new(),
        }
    }
}

impl Config {
    /// Path to the config file.
    fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        config_dir.join("cluihud").join("config.json")
    }

    /// Load config from disk, falling back to defaults.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }
}
