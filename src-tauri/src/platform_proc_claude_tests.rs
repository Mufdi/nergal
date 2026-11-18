use super::is_claude_process;
use std::path::Path;

fn cmd(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_process_named_claude_is_claude() {
    assert!(is_claude_process("claude", None, &[]));
}

#[test]
fn an_executable_under_a_claude_directory_is_claude() {
    let exe = Path::new("/home/u/.local/share/claude/versions/2.1.289");
    assert!(is_claude_process("2.1.289", Some(exe), &[]));
}

#[test]
fn node_running_a_claude_script_is_claude() {
    let argv = cmd(&[
        "node",
        "/usr/lib/node_modules/@anthropic-ai/claude-code/bin/claude",
    ]);
    assert!(is_claude_process("node", None, &argv));
}

#[test]
fn a_launcher_whose_first_arg_ends_in_claude_is_claude() {
    let argv = cmd(&["/home/u/.local/bin/claude", "--resume"]);
    assert!(is_claude_process("MainThread", None, &argv));
}

#[test]
fn shells_and_node_running_other_scripts_are_not_claude() {
    let exe = Path::new("/usr/bin/zsh");
    assert!(!is_claude_process("zsh", Some(exe), &cmd(&["zsh", "-l"])));
    assert!(!is_claude_process(
        "node",
        Some(Path::new("/usr/bin/node")),
        &cmd(&["node", "/home/u/app/server.js"])
    ));
}
