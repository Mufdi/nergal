use super::*;

#[test]
fn reading_flags_default_false_when_absent_and_round_trip() {
    let mut v = serde_json::to_value(Config::default()).unwrap();
    let obj = v.as_object_mut().unwrap();
    obj.remove("reading_mode_terminal");
    obj.remove("reading_mode_markdown");
    let back: Config = serde_json::from_value(v).unwrap();
    assert!(!back.reading_mode_terminal);
    assert!(!back.reading_mode_markdown);

    let on = Config {
        reading_mode_terminal: true,
        reading_mode_markdown: true,
        ..Config::default()
    };
    let back: Config = serde_json::from_str(&serde_json::to_string(&on).unwrap()).unwrap();
    assert!(back.reading_mode_terminal);
    assert!(back.reading_mode_markdown);
}

#[test]
fn full_width_tables_default_true_when_absent_and_round_trip_false() {
    let mut v = serde_json::to_value(Config::default()).unwrap();
    v.as_object_mut()
        .unwrap()
        .remove("reading_mode_full_width_tables");
    let back: Config = serde_json::from_value(v).unwrap();
    assert!(back.reading_mode_full_width_tables);

    let off = Config {
        reading_mode_full_width_tables: false,
        ..Config::default()
    };
    let back: Config = serde_json::from_str(&serde_json::to_string(&off).unwrap()).unwrap();
    assert!(!back.reading_mode_full_width_tables);
}
