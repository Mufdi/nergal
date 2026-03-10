use std::fs;
use std::path::PathBuf;

/// Parsed subset of ~/.config/ghostty/config relevant to cluihud.
#[derive(Debug, Default)]
pub struct GhosttyConfig {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
}

impl GhosttyConfig {
    /// Loads config from ~/.config/ghostty/config, returning defaults on any I/O error.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        Self::parse(&contents)
    }

    fn config_path() -> Option<PathBuf> {
        let config_dir = dirs::config_dir()?;
        let path = config_dir.join("ghostty").join("config");
        path.exists().then_some(path)
    }

    fn parse(contents: &str) -> Self {
        let mut config = Self::default();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match key {
                "font-family" => {
                    if !value.is_empty() {
                        config.font_family = Some(value.to_string());
                    }
                }
                "font-size" => {
                    if let Ok(size) = value.parse::<f32>() {
                        config.font_size = Some(size);
                    }
                }
                _ => {}
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let input = r#"
# Ghostty config
font-family = CaskaydiaMono NFP
font-size = 10
unknown-key = whatever
"#;
        let config = GhosttyConfig::parse(input);
        assert_eq!(config.font_family.as_deref(), Some("CaskaydiaMono NFP"));
        assert_eq!(config.font_size, Some(10.0));
    }

    #[test]
    fn parse_empty_config() {
        let config = GhosttyConfig::parse("");
        assert!(config.font_family.is_none());
        assert!(config.font_size.is_none());
    }
}
