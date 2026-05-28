use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::obsidian::config::ResolvedObsidianConfig;

#[derive(Debug, Clone, Serialize)]
pub struct Template {
    pub filename: String,
    pub name: String,
    pub description: Option<String>,
    pub body: String,
}

pub fn list_templates(cfg: &ResolvedObsidianConfig) -> Result<Vec<Template>> {
    let Some(dir) = cfg.templates_path.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    list_templates_from_dir(Path::new(dir))
}

pub fn list_templates_from_dir(dir: &Path) -> Result<Vec<Template>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !filename.starts_with("template-") || !filename.ends_with(".md") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("templates: skipping {}: {e}", path.display());
                continue;
            }
        };
        let (fm, body) = split_frontmatter(&raw);
        let derived_name = filename
            .trim_start_matches("template-")
            .trim_end_matches(".md")
            .replace('-', " ");
        let parsed = parse_frontmatter(fm);
        out.push(Template {
            filename: filename.to_string(),
            name: parsed.name.unwrap_or(derived_name),
            description: parsed.description,
            body: body.to_string(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

// Returns (frontmatter_body, content_body). When the file doesn't start with
// a `---` fence the whole text is content, frontmatter is empty.
fn split_frontmatter(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return ("", trimmed);
    }
    let after_open = match trimmed.strip_prefix("---") {
        Some(s) => s.trim_start_matches('\r').trim_start_matches('\n'),
        None => return ("", trimmed),
    };
    if let Some(end) = after_open.find("\n---") {
        let fm = &after_open[..end];
        let after_close = after_open[end + 4..]
            .trim_start_matches('\r')
            .trim_start_matches('\n');
        return (fm, after_close);
    }
    ("", trimmed)
}

#[derive(Default)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

fn parse_frontmatter(fm: &str) -> Frontmatter {
    let mut out = Frontmatter::default();
    for line in fm.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());
        match key {
            "name" => out.name = Some(value),
            "description" => out.description = Some(value),
            _ => {}
        }
    }
    out
}

fn unquote(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::config::ObsidianConfig;

    fn cfg_with_templates(dir: &str) -> ResolvedObsidianConfig {
        ObsidianConfig {
            vault_root: Some(dir.to_string()),
            templates_path: Some(dir.to_string()),
            ..ObsidianConfig::defaults()
        }
    }

    #[test]
    fn list_skips_when_unset() {
        let cfg = ObsidianConfig::defaults();
        let out = list_templates(&cfg).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn list_ignores_non_template_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("random.md"), "ignored").unwrap();
        std::fs::write(dir.path().join("template-yes.md"), "body").unwrap();
        let cfg = cfg_with_templates(dir.path().to_str().unwrap());
        let out = list_templates(&cfg).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].filename, "template-yes.md");
    }

    #[test]
    fn frontmatter_parsed() {
        let raw =
            "---\nname: \"TDD refactor\"\ndescription: 'fix the focused file'\n---\nbody here\n";
        let (fm, body) = split_frontmatter(raw);
        let parsed = parse_frontmatter(fm);
        assert_eq!(parsed.name.as_deref(), Some("TDD refactor"));
        assert_eq!(parsed.description.as_deref(), Some("fix the focused file"));
        assert_eq!(body, "body here\n");
    }

    #[test]
    fn no_frontmatter_uses_filename_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("template-explore-tradeoffs.md"),
            "the body\n",
        )
        .unwrap();
        let cfg = cfg_with_templates(dir.path().to_str().unwrap());
        let out = list_templates(&cfg).unwrap();
        assert_eq!(out[0].name, "explore tradeoffs");
        assert_eq!(out[0].body, "the body\n");
    }

    #[test]
    fn sorts_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("template-zebra.md"), "z").unwrap();
        std::fs::write(dir.path().join("template-apple.md"), "a").unwrap();
        let cfg = cfg_with_templates(dir.path().to_str().unwrap());
        let out = list_templates(&cfg).unwrap();
        assert_eq!(out[0].name, "apple");
        assert_eq!(out[1].name, "zebra");
    }

    #[test]
    fn frontmatter_with_no_close_treats_all_as_content() {
        let raw = "---\nname: oops\nno-close-fence\nbody\n";
        let (fm, body) = split_frontmatter(raw);
        assert!(fm.is_empty());
        assert!(body.contains("body"));
    }

    #[test]
    fn list_from_dir_returns_empty_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let out = list_templates_from_dir(&missing).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn list_from_dir_returns_empty_when_path_is_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("not-a-dir.md");
        std::fs::write(&file_path, "body").unwrap();
        let out = list_templates_from_dir(&file_path).unwrap();
        assert!(out.is_empty());
    }
}
