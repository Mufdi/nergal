use serde_json::Value;
use std::collections::BTreeSet;

const CONFIG: &str = include_str!("../tauri.conf.json");

const EXPECTED_CONNECT_SRC: [&str; 3] = ["'self'", "ipc:", "http://ipc.localhost"];

const EXPECTED_OTHER_DIRECTIVES: [(&str, &str); 8] = [
    ("default-src", "'self'"),
    ("script-src", "'self'"),
    ("img-src", "'self' data: https:"),
    ("font-src", "'self'"),
    ("style-src", "'self' 'unsafe-inline'"),
    ("frame-src", "http: https:"),
    ("base-uri", "'self'"),
    ("form-action", "'self'"),
];

fn policy(key: &str) -> Vec<(String, String)> {
    let config: Value = serde_json::from_str(CONFIG).expect("tauri.conf.json is valid JSON");
    let csp = config["app"]["security"][key]
        .as_str()
        .unwrap_or_else(|| panic!("app.security.{key} is a string"));
    csp.split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| match d.split_once(' ') {
            Some((name, sources)) => (name.to_owned(), sources.to_owned()),
            None => (d.to_owned(), String::new()),
        })
        .collect()
}

fn connect_src_sources(key: &str) -> Vec<String> {
    let directives = policy(key);
    let (_, sources) = directives
        .iter()
        .find(|(name, _)| name == "connect-src")
        .unwrap_or_else(|| panic!("{key} has a connect-src directive"));
    sources.split_whitespace().map(str::to_owned).collect()
}

fn assert_connect_src_allows_tauri_ipc(key: &str) {
    let sources = connect_src_sources(key);
    let got: BTreeSet<&str> = sources.iter().map(String::as_str).collect();
    let want: BTreeSet<&str> = EXPECTED_CONNECT_SRC.into_iter().collect();
    assert_eq!(
        sources.len(),
        want.len(),
        "{key} connect-src has duplicates or extras: {sources:?}"
    );
    assert_eq!(got, want, "{key} connect-src must be exactly {want:?}");
}

fn assert_other_directives_unchanged(key: &str) {
    let directives = policy(key);
    assert_eq!(
        directives.len(),
        EXPECTED_OTHER_DIRECTIVES.len() + 1,
        "{key} must keep exactly its current directive set: {directives:?}"
    );
    for (name, sources) in EXPECTED_OTHER_DIRECTIVES {
        let found = directives.iter().find(|(n, _)| n == name);
        assert_eq!(
            found.map(|(_, s)| s.as_str()),
            Some(sources),
            "{key} directive {name} changed"
        );
    }
}

#[test]
fn csp_connect_src_allows_tauri_ipc_custom_protocol() {
    assert_connect_src_allows_tauri_ipc("csp");
}

#[test]
fn dev_csp_connect_src_allows_tauri_ipc_custom_protocol() {
    assert_connect_src_allows_tauri_ipc("devCsp");
}

#[test]
fn csp_keeps_every_other_directive_unchanged() {
    assert_other_directives_unchanged("csp");
}

#[test]
fn dev_csp_keeps_every_other_directive_unchanged() {
    assert_other_directives_unchanged("devCsp");
}
