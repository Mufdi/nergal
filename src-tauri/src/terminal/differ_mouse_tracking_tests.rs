use super::*;

fn snapshot(mouse_tracking: bool) -> GridSnapshot {
    GridSnapshot {
        cols: 3,
        rows: vec![vec![CellSnapshot::default(); 3]; 2],
        cursor: CursorSnapshot {
            x: 0,
            y: 0,
            visible: true,
        },
        title: None,
        scroll_offset: 0,
        is_alt_screen: false,
        mouse_tracking,
    }
}

fn primed(mouse_tracking: bool) -> GridDiffer {
    let mut differ = GridDiffer::new();
    assert!(
        differ
            .compute_update("s", &snapshot(mouse_tracking))
            .is_some()
    );
    differ
}

#[test]
fn flip_alone_emits_an_update_with_the_new_value() {
    let mut differ = primed(false);
    let update = differ
        .compute_update("s", &snapshot(true))
        .expect("a mouse_tracking flip must emit");
    assert!(update.mouse_tracking);
    assert!(update.rows.is_empty());
}

#[test]
fn unchanged_mouse_tracking_emits_nothing() {
    let mut differ = primed(true);
    assert!(differ.compute_update("s", &snapshot(true)).is_none());
}

#[test]
fn h_l_h_with_no_other_change_emits_three_times() {
    let mut differ = primed(false);
    let seen: Vec<bool> = [true, false, true]
        .into_iter()
        .map(|on| {
            differ
                .compute_update("s", &snapshot(on))
                .expect("each flip must emit")
                .mouse_tracking
        })
        .collect();
    assert_eq!(seen, vec![true, false, true]);
}

#[test]
fn invalidate_makes_the_next_update_carry_the_current_value() {
    let mut differ = primed(true);
    differ.invalidate();
    let update = differ
        .compute_update("s", &snapshot(true))
        .expect("invalidate forces a full emission");
    assert!(update.mouse_tracking);
}
