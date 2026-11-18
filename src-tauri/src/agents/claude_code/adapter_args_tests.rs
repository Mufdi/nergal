use super::cc_resume_args;

#[test]
fn resume_from_maps_to_the_cli_flags_without_touching_the_path() {
    let cases: [(&str, Option<&str>, Vec<&str>); 4] = [
        ("no resume", None, vec![]),
        // `--continue` would resume the newest conversation of the cwd, which can belong to another session.
        ("continue is never forwarded", Some("continue"), vec![]),
        ("resume_pick", Some("resume_pick"), vec!["--resume"]),
        (
            "explicit id",
            Some("aaaa-1111"),
            vec!["--resume", "aaaa-1111"],
        ),
    ];
    for (name, resume_from, expected) in cases {
        let expected: Vec<String> = expected.into_iter().map(String::from).collect();
        assert_eq!(cc_resume_args(resume_from), expected, "{name}");
    }
}
