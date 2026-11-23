#![cfg(unix)]

use super::{ipc_dir, plan_review_fifo_path};

#[test]
fn plan_review_fifo_paths_are_unique_per_call_in_ipc_dir() {
    let a = plan_review_fifo_path(42).unwrap();
    let b = plan_review_fifo_path(42).unwrap();
    let ipc = ipc_dir().unwrap();
    assert_eq!(a.parent().unwrap(), ipc.as_path());
    assert_eq!(b.parent().unwrap(), ipc.as_path());
    for p in [&a, &b] {
        let name = p.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("plan-42-"), "name was {name}");
        assert!(name.ends_with(".fifo"), "name was {name}");
    }
    assert_ne!(a, b);
}
