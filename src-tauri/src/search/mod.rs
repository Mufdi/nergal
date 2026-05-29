//! Global search engine (obsidian-bridge M2 infrastructure).
//!
//! A scope-filtered text search reused by every "find across X" surface:
//! #7 (Ask the vault) runs it with `scopes: [Vault]`, #I (`@@` picker) with
//! `scopes: [Vault], titles_only: true`. A future global Cmd+P inherits the
//! same engine by adding a scope variant instead of re-architecting.
//!
//! Fast path shells out to ripgrep (`rg --json`); when `rg` is absent from
//! PATH the engine falls back to a pure-Rust `walkdir` scan so it stays
//! self-contained. ripgrep is the optimization, not the dependency.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Relevance weights. A hit accumulates these: a content line match starts at
/// `CONTENT`, gains `FILENAME` when the file's basename contains the query, and
/// `TITLE` when the note's frontmatter/H1 title does. Filename-only hits
/// (titles_only mode) score `FILENAME`.
const SCORE_FILENAME: i64 = 100;
const SCORE_TITLE: i64 = 50;
const SCORE_CONTENT: i64 = 10;

/// Directories never worth walking inside a workspace repo. ripgrep already
/// respects `.gitignore` + skips `.git`; these globs cover the common cases
/// that are frequently *not* gitignored, and gate the walkdir fallback too.
const REPO_EXCLUDES: &[&str] = &[".git", "node_modules", "target", ".venv"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    pub scopes: Vec<SearchScope>,
    #[serde(default)]
    pub case_sensitive: bool,
    /// When true, match only against file names / note titles (no content
    /// grep). Powers the `@@` mention picker.
    #[serde(default)]
    pub titles_only: bool,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    50
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchScope {
    Vault,
    SessionTranscripts,
    OpenSpec,
    #[serde(rename_all = "camelCase")]
    WorkspaceFiles {
        workspace_id: String,
    },
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    /// `None` for filename/title-only hits.
    pub line_number: Option<u64>,
    /// Matched content line, or the note title in titles_only mode.
    pub line_text: String,
    pub scope: String,
    pub score: i64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchContext {
    pub vault_root: Option<PathBuf>,
    pub transcripts_dir: Option<PathBuf>,
    pub openspec_dir: Option<PathBuf>,
    /// workspace_id -> repo_path
    pub workspace_paths: HashMap<String, PathBuf>,
}

struct ScopePlan {
    label: &'static str,
    root: PathBuf,
    markdown_only: bool,
    excludes: &'static [&'static str],
}

pub struct SearchEngine;

impl SearchEngine {
    pub fn search(query: &SearchQuery, ctx: &SearchContext) -> Result<Vec<SearchHit>> {
        let needle = query.text.trim();
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        let plans = resolve_plans(&query.scopes, ctx);
        let rg_available = which::which("rg").is_ok();

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut title_cache: HashMap<PathBuf, Option<String>> = HashMap::new();

        for plan in &plans {
            if query.titles_only {
                collect_title_hits(plan, needle, query, &mut hits);
            } else if rg_available {
                collect_ripgrep_hits(plan, needle, query, &mut hits, &mut title_cache)?;
            } else {
                collect_walkdir_hits(plan, needle, query, &mut hits, &mut title_cache);
            }
            if hits.len() >= query.max_results.saturating_mul(4) {
                // Bound work on pathological vaults; final sort+truncate below.
                break;
            }
        }

        // The consuming surfaces (#7 modal, #I picker) list files, not lines:
        // collapse multiple line matches in one file to its best-scoring hit.
        let mut best: HashMap<String, SearchHit> = HashMap::new();
        for hit in hits {
            match best.get(&hit.path) {
                Some(existing)
                    if existing.score > hit.score
                        || (existing.score == hit.score
                            && existing.line_number <= hit.line_number) => {}
                _ => {
                    best.insert(hit.path.clone(), hit);
                }
            }
        }
        let mut hits: Vec<SearchHit> = best.into_values().collect();

        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        hits.truncate(query.max_results);
        Ok(hits)
    }
}

fn resolve_plans(scopes: &[SearchScope], ctx: &SearchContext) -> Vec<ScopePlan> {
    let mut plans = Vec::new();
    let push_scope = |scope: &SearchScope, plans: &mut Vec<ScopePlan>| match scope {
        SearchScope::Vault => {
            if let Some(root) = &ctx.vault_root {
                plans.push(ScopePlan {
                    label: "vault",
                    root: root.clone(),
                    markdown_only: true,
                    excludes: &[],
                });
            }
        }
        SearchScope::SessionTranscripts => {
            if let Some(root) = &ctx.transcripts_dir {
                plans.push(ScopePlan {
                    label: "transcripts",
                    root: root.clone(),
                    markdown_only: false,
                    excludes: &[],
                });
            }
        }
        SearchScope::OpenSpec => {
            if let Some(root) = &ctx.openspec_dir {
                plans.push(ScopePlan {
                    label: "openspec",
                    root: root.clone(),
                    markdown_only: true,
                    excludes: &[],
                });
            }
        }
        SearchScope::WorkspaceFiles { workspace_id } => {
            if let Some(root) = ctx.workspace_paths.get(workspace_id) {
                plans.push(ScopePlan {
                    label: "files",
                    root: root.clone(),
                    markdown_only: false,
                    excludes: REPO_EXCLUDES,
                });
            }
        }
        SearchScope::All => {}
    };

    for scope in scopes {
        if matches!(scope, SearchScope::All) {
            push_scope(&SearchScope::Vault, &mut plans);
            push_scope(&SearchScope::SessionTranscripts, &mut plans);
            push_scope(&SearchScope::OpenSpec, &mut plans);
            for workspace_id in ctx.workspace_paths.keys() {
                push_scope(
                    &SearchScope::WorkspaceFiles {
                        workspace_id: workspace_id.clone(),
                    },
                    &mut plans,
                );
            }
        } else {
            push_scope(scope, &mut plans);
        }
    }
    plans
}

/// ripgrep JSON match record (only the fields we read). One JSON object per
/// line on stdout; we ignore `begin` / `end` / `summary` record types.
#[derive(Deserialize)]
struct RgEnvelope {
    #[serde(rename = "type")]
    kind: String,
    data: Option<RgData>,
}

#[derive(Deserialize)]
struct RgData {
    path: Option<RgText>,
    lines: Option<RgText>,
    line_number: Option<u64>,
}

#[derive(Deserialize)]
struct RgText {
    text: Option<String>,
}

fn collect_ripgrep_hits(
    plan: &ScopePlan,
    needle: &str,
    query: &SearchQuery,
    hits: &mut Vec<SearchHit>,
    title_cache: &mut HashMap<PathBuf, Option<String>>,
) -> Result<()> {
    if !plan.root.exists() {
        return Ok(());
    }

    let mut cmd = Command::new("rg");
    cmd.arg("--json")
        .arg("--line-number")
        .arg("--fixed-strings")
        .arg("--max-count")
        .arg(query.max_results.to_string());
    if !query.case_sensitive {
        cmd.arg("-i");
    }
    if plan.markdown_only {
        cmd.arg("--glob").arg("*.md");
    }
    for ex in plan.excludes {
        cmd.arg("--glob").arg(format!("!{ex}/"));
    }
    cmd.arg("--").arg(needle).arg(&plan.root);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("rg produced no stdout handle"))?;
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let env: RgEnvelope = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if env.kind != "match" {
            continue;
        }
        let Some(data) = env.data else { continue };
        let Some(path) = data.path.and_then(|p| p.text) else {
            continue;
        };
        let line_text = data
            .lines
            .and_then(|l| l.text)
            .unwrap_or_default()
            .trim_end_matches(['\n', '\r'])
            .to_string();
        hits.push(score_content_hit(
            plan,
            &path,
            data.line_number,
            line_text,
            needle,
            query.case_sensitive,
            title_cache,
        ));
    }

    let _ = child.wait();
    Ok(())
}

fn collect_walkdir_hits(
    plan: &ScopePlan,
    needle: &str,
    query: &SearchQuery,
    hits: &mut Vec<SearchHit>,
    title_cache: &mut HashMap<PathBuf, Option<String>>,
) {
    let needle_cmp = if query.case_sensitive {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };

    for entry in walkdir::WalkDir::new(&plan.root)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e, plan.excludes))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if plan.markdown_only && path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (idx, raw) in content.lines().enumerate() {
            let haystack = if query.case_sensitive {
                raw.to_string()
            } else {
                raw.to_lowercase()
            };
            if haystack.contains(&needle_cmp) {
                hits.push(score_content_hit(
                    plan,
                    &path.to_string_lossy(),
                    Some(idx as u64 + 1),
                    raw.trim_end().to_string(),
                    needle,
                    query.case_sensitive,
                    title_cache,
                ));
                break; // first match per file mirrors rg --max-count bounding
            }
        }
    }
}

fn collect_title_hits(
    plan: &ScopePlan,
    needle: &str,
    query: &SearchQuery,
    hits: &mut Vec<SearchHit>,
) {
    let needle_cmp = if query.case_sensitive {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };

    for entry in walkdir::WalkDir::new(&plan.root)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e, plan.excludes))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        // titles_only is a note picker: markdown notes only, regardless of scope.
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem_cmp = if query.case_sensitive {
            stem.clone()
        } else {
            stem.to_lowercase()
        };
        let frontmatter_title = read_title(path);
        let title_matches = frontmatter_title
            .as_deref()
            .map(|t| {
                let t = if query.case_sensitive {
                    t.to_string()
                } else {
                    t.to_lowercase()
                };
                t.contains(&needle_cmp)
            })
            .unwrap_or(false);

        if stem_cmp.contains(&needle_cmp) {
            hits.push(SearchHit {
                path: path.to_string_lossy().into_owned(),
                line_number: None,
                line_text: frontmatter_title.clone().unwrap_or_else(|| stem.clone()),
                scope: plan.label.into(),
                score: SCORE_FILENAME,
                title: frontmatter_title,
            });
        } else if title_matches {
            hits.push(SearchHit {
                path: path.to_string_lossy().into_owned(),
                line_number: None,
                line_text: frontmatter_title.clone().unwrap_or_else(|| stem.clone()),
                scope: plan.label.into(),
                score: SCORE_TITLE,
                title: frontmatter_title,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn score_content_hit(
    plan: &ScopePlan,
    path: &str,
    line_number: Option<u64>,
    line_text: String,
    needle: &str,
    case_sensitive: bool,
    title_cache: &mut HashMap<PathBuf, Option<String>>,
) -> SearchHit {
    let pb = PathBuf::from(path);
    let needle_cmp = if case_sensitive {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };

    let mut score = SCORE_CONTENT;

    let basename = pb
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let basename_cmp = if case_sensitive {
        basename.clone()
    } else {
        basename.to_lowercase()
    };
    if basename_cmp.contains(&needle_cmp) {
        score += SCORE_FILENAME;
    }

    let title = title_cache
        .entry(pb.clone())
        .or_insert_with(|| read_title(&pb))
        .clone();
    if let Some(t) = &title {
        let t_cmp = if case_sensitive {
            t.clone()
        } else {
            t.to_lowercase()
        };
        if t_cmp.contains(&needle_cmp) {
            score += SCORE_TITLE;
        }
    }

    SearchHit {
        path: path.to_string(),
        line_number,
        line_text,
        scope: plan.label.into(),
        score,
        title,
    }
}

fn is_excluded_dir(entry: &walkdir::DirEntry, excludes: &[&str]) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|n| excludes.contains(&n))
        .unwrap_or(false)
}

/// Cheap title extraction: frontmatter `title:` field, else the first `# H1`.
/// Reads only the file head (markdown notes put titles near the top).
fn read_title(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut in_frontmatter = false;
    let mut first_line = true;
    for (idx, line) in reader.lines().enumerate().take(40) {
        let line = line.ok()?;
        let trimmed = line.trim();
        if first_line {
            first_line = false;
            if trimmed == "---" {
                in_frontmatter = true;
                continue;
            }
        }
        if in_frontmatter {
            if trimmed == "---" {
                in_frontmatter = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("title:") {
                let val = rest.trim().trim_matches(['"', '\'']).to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let val = rest.trim().to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
        if idx > 38 {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests;
