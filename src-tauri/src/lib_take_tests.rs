use super::ReconciledWorktrees;
use std::sync::Mutex;

#[test]
fn the_names_are_handed_out_once_then_the_list_is_empty() {
    let state = ReconciledWorktrees(Mutex::new(vec![
        "Fixes generales".into(),
        "Upgrades".into(),
    ]));

    assert_eq!(state.take(), vec!["Fixes generales", "Upgrades"]);
    assert_eq!(state.take(), Vec::<String>::new());
}
