use super::*;
use std::collections::HashMap;
use std::fs;

fn vault_ctx(root: &Path) -> SearchContext {
    SearchContext {
        vault_root: Some(root.to_path_buf()),
        ..Default::default()
    }
}

fn vault_query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.to_string(),
        scopes: vec![SearchScope::Vault],
        case_sensitive: false,
        titles_only: false,
        max_results: 50,
    }
}

#[test]
fn empty_query_returns_no_hits() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "hello world").unwrap();
    let hits = SearchEngine::search(&vault_query("   "), &vault_ctx(dir.path())).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn content_match_scores_above_unmatched() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("random.md"),
        "a paragraph mentioning ripgrep somewhere",
    )
    .unwrap();
    let hits = SearchEngine::search(&vault_query("ripgrep"), &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].score, SCORE_CONTENT);
    assert_eq!(hits[0].scope, "vault");
    assert!(hits[0].line_number.is_some());
}

#[test]
fn filename_match_outranks_content_only() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("ripgrep.md"), "this note mentions ripgrep").unwrap();
    fs::write(dir.path().join("other.md"), "mentions ripgrep in body").unwrap();
    let hits = SearchEngine::search(&vault_query("ripgrep"), &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits[0].path.ends_with("ripgrep.md"));
    assert!(hits[0].score > hits[1].score);
    assert!(hits[0].score >= SCORE_FILENAME);
}

#[test]
fn frontmatter_title_match_adds_title_score() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("n.md"),
        "---\ntitle: Searching the Vault\n---\n\nbody has searching keyword\n",
    )
    .unwrap();
    let hits = SearchEngine::search(&vault_query("searching"), &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title.as_deref(), Some("Searching the Vault"));
    assert_eq!(hits[0].score, SCORE_CONTENT + SCORE_TITLE);
}

#[test]
fn markdown_only_scope_skips_non_md() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "needle here").unwrap();
    fs::write(dir.path().join("data.json"), "needle here too").unwrap();
    let hits = SearchEngine::search(&vault_query("needle"), &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].path.ends_with("note.md"));
}

#[test]
fn titles_only_matches_filename_not_body() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Architecture.md"), "no keyword in here").unwrap();
    fs::write(
        dir.path().join("other.md"),
        "architecture discussed in body",
    )
    .unwrap();
    let mut q = vault_query("architecture");
    q.titles_only = true;
    let hits = SearchEngine::search(&q, &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].path.ends_with("Architecture.md"));
    assert_eq!(hits[0].score, SCORE_FILENAME);
    assert!(hits[0].line_number.is_none());
}

#[test]
fn titles_only_matches_frontmatter_title() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("cryptic-slug.md"),
        "---\ntitle: Payment Gateway\n---\nbody\n",
    )
    .unwrap();
    let mut q = vault_query("payment");
    q.titles_only = true;
    let hits = SearchEngine::search(&q, &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].score, SCORE_TITLE);
    assert_eq!(hits[0].title.as_deref(), Some("Payment Gateway"));
}

#[test]
fn case_sensitive_excludes_wrong_case() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "Needle uppercase").unwrap();
    let mut q = vault_query("needle");
    q.case_sensitive = true;
    let hits = SearchEngine::search(&q, &vault_ctx(dir.path())).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn max_results_caps_output() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..10 {
        fs::write(dir.path().join(format!("n{i}.md")), "common keyword").unwrap();
    }
    let mut q = vault_query("common");
    q.max_results = 3;
    let hits = SearchEngine::search(&q, &vault_ctx(dir.path())).unwrap();
    assert_eq!(hits.len(), 3);
}

#[test]
fn workspace_scope_prunes_excluded_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("node_modules").join("pkg");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("index.js"), "secret token here").unwrap();
    fs::write(dir.path().join("src.rs"), "secret token here").unwrap();

    let mut ws = HashMap::new();
    ws.insert("w1".to_string(), dir.path().to_path_buf());
    let ctx = SearchContext {
        workspace_paths: ws,
        ..Default::default()
    };
    let q = SearchQuery {
        text: "secret".to_string(),
        scopes: vec![SearchScope::WorkspaceFiles {
            workspace_id: "w1".to_string(),
        }],
        case_sensitive: false,
        titles_only: false,
        max_results: 50,
    };
    let hits = SearchEngine::search(&q, &ctx).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].path.ends_with("src.rs"));
}

#[test]
fn missing_scope_root_is_silent() {
    let ctx = SearchContext::default();
    let hits = SearchEngine::search(&vault_query("anything"), &ctx).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn read_title_prefers_frontmatter_over_h1() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("n.md");
    fs::write(&p, "---\ntitle: From Frontmatter\n---\n# An H1 Heading\n").unwrap();
    assert_eq!(read_title(&p).as_deref(), Some("From Frontmatter"));
}

#[test]
fn read_title_falls_back_to_h1() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("n.md");
    fs::write(&p, "# Just A Heading\n\nbody\n").unwrap();
    assert_eq!(read_title(&p).as_deref(), Some("Just A Heading"));
}
