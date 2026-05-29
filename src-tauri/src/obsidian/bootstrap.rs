use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::obsidian::config::ResolvedObsidianConfig;

pub struct ProjectNoteOutcome {
    pub path: PathBuf,
    pub created: bool,
}

pub fn create_project_note(
    cfg: &ResolvedObsidianConfig,
    workspace_name: &str,
    workspace_path: &Path,
) -> Result<ProjectNoteOutcome> {
    let vault_root = cfg
        .vault_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("vault_root is not configured"))?;
    let target = Path::new(vault_root)
        .join("Projects")
        .join(slugify_for_vault(workspace_name))
        .join("index.md");
    create_project_note_at(&target, workspace_name, workspace_path)
}

pub fn create_project_note_at(
    target: &Path,
    workspace_name: &str,
    workspace_path: &Path,
) -> Result<ProjectNoteOutcome> {
    if target.exists() {
        return Ok(ProjectNoteOutcome {
            path: target.to_path_buf(),
            created: false,
        });
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let body = render_template(workspace_name, workspace_path);
    std::fs::write(target, body).with_context(|| format!("writing {}", target.display()))?;
    Ok(ProjectNoteOutcome {
        path: target.to_path_buf(),
        created: true,
    })
}

pub fn suggested_layout_paths(
    cfg: &ResolvedObsidianConfig,
    workspace_name: &str,
) -> Result<(String, String)> {
    let vault_root = cfg
        .vault_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("vault_root is not configured"))?;
    let slug = slugify_for_vault(workspace_name);
    let project_dir = Path::new(vault_root).join("Projects").join(&slug);
    let log = project_dir.join("log.md");
    let mocs = project_dir.join("MOCs");
    if log.to_str().is_none() || mocs.to_str().is_none() {
        bail!("non-utf8 path produced from vault_root");
    }
    Ok((log.display().to_string(), mocs.display().to_string()))
}

fn render_template(name: &str, workspace_path: &Path) -> String {
    let abs = workspace_path.display().to_string();
    let encoded = encode_uri_component(&abs);
    let link = format!("cluihud://open-workspace?path={encoded}");
    format!(
        "# {name}\n\n## Workspace\n\n[Open in Nergal]({link})\n\n`{abs}`\n\n## Decisions\n\n## Log\n"
    )
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// File-system safe slug: keep alphanumerics, hyphens, underscores, dots; the
// vault uses these as directory names so spaces and other punctuation are
// flattened to dashes. Diacritics are stripped to avoid case-sensitivity
// headaches across rsync/dropbox/git-on-mac surfaces of the vault.
pub(crate) fn slugify_for_vault(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let mapped = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            c => c,
        };
        if mapped.is_alphanumeric() || mapped == '-' || mapped == '_' || mapped == '.' {
            out.push(mapped);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::config::ObsidianConfig;

    fn cfg_with_vault(root: &str) -> ResolvedObsidianConfig {
        ObsidianConfig {
            vault_root: Some(root.into()),
            ..ObsidianConfig::defaults()
        }
    }

    #[test]
    fn slug_strips_diacritics_and_spaces() {
        assert_eq!(slugify_for_vault("Niño Volador"), "Nino-Volador");
        assert_eq!(slugify_for_vault("é à ñ"), "e-a-n");
    }

    #[test]
    fn slug_trims_leading_trailing_dashes() {
        assert_eq!(slugify_for_vault(" foo bar "), "foo-bar");
        assert_eq!(slugify_for_vault("!!!hello!!!"), "hello");
    }

    #[test]
    fn create_writes_template_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_with_vault(dir.path().to_str().unwrap());
        let out = create_project_note(&cfg, "demo", Path::new("/home/user/demo")).unwrap();
        assert!(out.created);
        assert!(out.path.exists());
        let body = std::fs::read_to_string(&out.path).unwrap();
        assert!(body.contains("# demo"));
        assert!(body.contains("`/home/user/demo`"));
        assert!(body.contains("cluihud://open-workspace?path=%2Fhome%2Fuser%2Fdemo"));
        assert!(body.contains("[Open in Nergal]"));
        assert!(body.contains("## Workspace"));
        assert!(body.contains("## Decisions"));
        assert!(body.contains("## Log"));
    }

    #[test]
    fn encode_uri_component_preserves_unreserved() {
        assert_eq!(encode_uri_component("abc-DEF_123.~"), "abc-DEF_123.~");
    }

    #[test]
    fn encode_uri_component_percent_encodes_slashes_and_spaces() {
        assert_eq!(
            encode_uri_component("/home/user/my project"),
            "%2Fhome%2Fuser%2Fmy%20project"
        );
    }

    #[test]
    fn create_skips_overwrite_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_with_vault(dir.path().to_str().unwrap());
        let project_dir = dir.path().join("Projects").join("demo");
        std::fs::create_dir_all(&project_dir).unwrap();
        let target = project_dir.join("index.md");
        std::fs::write(&target, "pre-existing").unwrap();
        let out = create_project_note(&cfg, "demo", Path::new("/x")).unwrap();
        assert!(!out.created);
        assert_eq!(std::fs::read_to_string(&out.path).unwrap(), "pre-existing");
    }

    #[test]
    fn suggested_paths_under_project_dir() {
        let cfg = cfg_with_vault("/home/user/Vault");
        let (log, mocs) = suggested_layout_paths(&cfg, "demo").unwrap();
        assert_eq!(log, "/home/user/Vault/Projects/demo/log.md");
        assert_eq!(mocs, "/home/user/Vault/Projects/demo/MOCs");
    }
}
