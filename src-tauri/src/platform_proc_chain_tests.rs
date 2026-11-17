#![cfg(target_os = "linux")]
use super::agent_chain;

#[test]
fn the_chain_of_this_process_is_complete_and_starts_at_its_parent() {
    let chain = agent_chain();

    assert!(chain.complete);
    assert_eq!(chain.entries[0].0, std::os::unix::process::parent_id());
}
