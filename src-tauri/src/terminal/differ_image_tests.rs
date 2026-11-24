use super::*;
use crate::terminal::types::ImageRun;

fn run(hash: &str, u1: f32) -> ImageRun {
    ImageRun {
        hash: hash.repeat(64),
        col: 0,
        span: 3,
        u0: 0.0,
        v0: 0.0,
        u1,
        v1: 1.0,
        pad_left: 0,
        pad_top: 0,
        pad_right: 0,
        pad_bottom: 0,
        z: 0,
    }
}

fn snapshot(row0_runs: Vec<ImageRun>) -> GridSnapshot {
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
        mouse_tracking: false,
        images: vec![row0_runs, vec![]],
    }
}

fn primed(row0_runs: Vec<ImageRun>) -> GridDiffer {
    let mut differ = GridDiffer::new();
    assert!(differ.compute_update("s", &snapshot(row0_runs)).is_some());
    differ
}

#[test]
fn row_whose_runs_change_with_equal_cells_is_resent_alone() {
    let mut differ = primed(vec![run("a", 1.0)]);
    let update = differ
        .compute_update("s", &snapshot(vec![run("b", 1.0)]))
        .expect("changed runs must emit");

    assert_eq!(update.rows.len(), 1);
    assert_eq!(update.rows[0].index, 0);
    assert_eq!(update.rows[0].images, vec![run("b", 1.0)]);
}

#[test]
fn row_whose_slice_range_changes_is_resent() {
    let mut differ = primed(vec![run("a", 1.0)]);
    let update = differ
        .compute_update("s", &snapshot(vec![run("a", 0.5)]))
        .expect("changed u range must emit");
    assert_eq!(update.rows.len(), 1);
}

#[test]
fn row_with_unchanged_runs_is_not_resent() {
    let mut differ = primed(vec![run("a", 1.0)]);
    assert!(
        differ
            .compute_update("s", &snapshot(vec![run("a", 1.0)]))
            .is_none()
    );
}

#[test]
fn grid_row_json_omits_images_when_empty() {
    let row = GridRow {
        index: 0,
        cells: vec![],
        images: vec![],
    };
    let json = serde_json::to_value(&row).unwrap();
    assert!(json.get("images").is_none(), "{json}");
}

#[test]
fn grid_row_json_includes_camel_case_images_when_present() {
    let row = GridRow {
        index: 0,
        cells: vec![],
        images: vec![run("a", 1.0)],
    };
    let json = serde_json::to_value(&row).unwrap();
    let image = &json["images"][0];
    assert_eq!(image["hash"], "a".repeat(64));
    assert_eq!(image["col"], 0);
    assert_eq!(image["span"], 3);
    for key in [
        "u0",
        "v0",
        "u1",
        "v1",
        "padLeft",
        "padTop",
        "padRight",
        "padBottom",
        "z",
    ] {
        assert!(image.get(key).is_some(), "missing {key}: {image}");
    }
}
