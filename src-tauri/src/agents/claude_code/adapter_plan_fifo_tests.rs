use super::ClaudeCodeAdapter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn claim_with_nothing_registered_is_false() {
    let a = ClaudeCodeAdapter::new();
    assert!(!a.claim_pending_plan_fifo("s1", Path::new("/p/a")));
}

#[test]
fn claim_with_other_path_is_false_and_keeps_entry() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo("s1", PathBuf::from("/p/a"));
    assert!(!a.claim_pending_plan_fifo("s1", Path::new("/p/other")));
    assert!(a.claim_pending_plan_fifo("s1", Path::new("/p/a")));
}

#[test]
fn claim_with_same_path_removes_entry() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo("s1", PathBuf::from("/p/a"));
    assert!(a.claim_pending_plan_fifo("s1", Path::new("/p/a")));
    assert!(!a.claim_pending_plan_fifo("s1", Path::new("/p/a")));
}

#[test]
fn concurrent_claims_yield_exactly_one_winner() {
    for _ in 0..50 {
        let a = Arc::new(ClaudeCodeAdapter::new());
        a.register_pending_plan_fifo("s1", PathBuf::from("/p/a"));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let a = Arc::clone(&a);
                std::thread::spawn(move || a.claim_pending_plan_fifo("s1", Path::new("/p/a")))
            })
            .collect();
        let wins = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|w| *w)
            .count();
        assert_eq!(wins, 1);
    }
}

#[test]
fn register_returns_none_first_then_the_replaced_path() {
    let a = ClaudeCodeAdapter::new();
    assert_eq!(
        a.register_pending_plan_fifo("s1", PathBuf::from("/p/a")),
        None
    );
    assert_eq!(
        a.register_pending_plan_fifo("s1", PathBuf::from("/p/b")),
        Some(PathBuf::from("/p/a"))
    );
}

#[test]
fn release_returns_and_removes_the_registered_path() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo("s1", PathBuf::from("/p/a"));
    assert_eq!(
        a.release_pending_plan_fifo("s1"),
        Some(PathBuf::from("/p/a"))
    );
    assert!(!a.claim_pending_plan_fifo("s1", Path::new("/p/a")));
}

#[test]
fn release_with_nothing_registered_is_none() {
    let a = ClaudeCodeAdapter::new();
    assert_eq!(a.release_pending_plan_fifo("s1"), None);
}
