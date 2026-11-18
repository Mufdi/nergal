use super::rows_to_delete_on_partial;

#[test]
fn partial_workspace_delete_keeps_repo_root_sessions() {
    let outcomes = vec![
        ("root".to_string(), false, false),
        ("clean-wt".to_string(), true, false),
        ("failed-wt".to_string(), true, true),
    ];

    assert_eq!(
        rows_to_delete_on_partial(&outcomes),
        vec!["clean-wt".to_string()]
    );
}

#[test]
fn partial_workspace_delete_keeps_every_failed_worktree_session() {
    let outcomes = vec![("a".to_string(), true, true), ("b".to_string(), true, true)];

    assert!(rows_to_delete_on_partial(&outcomes).is_empty());
}
