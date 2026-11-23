use super::*;
use crate::agents::{LaunchPrompt, PromptAttach};
use crate::config::ShellKind;
use std::path::Path;

#[cfg(unix)]
mod unix_host {
    use super::*;

    fn hello() -> LaunchPrompt {
        LaunchPrompt {
            text: "hello".to_string(),
            attach: PromptAttach::AfterDoubleDash,
        }
    }

    // The POSIX file-reference trick does not exist in these shells, and the
    // inline form is the quoting bug class this change removes, so a unix host
    // must refuse instead of falling back.
    #[test]
    fn a_non_posix_shell_on_a_unix_host_refuses_a_launch_prompt_and_writes_no_file() {
        for kind in [ShellKind::PowerShell, ShellKind::Cmd] {
            let dir = tempfile::tempdir().unwrap();

            let result = prepare_launch(
                kind,
                "/work",
                None,
                Path::new("claude"),
                &[],
                Some(&hello()),
                Ok(dir.path().to_path_buf()),
                "pty-ps",
            );

            assert!(result.is_err(), "{kind:?} accepted a prompt");
            assert!(
                std::fs::read_dir(dir.path()).unwrap().next().is_none(),
                "{kind:?} left a file behind"
            );
        }
    }

    #[test]
    fn powershell_without_a_prompt_still_builds_the_unchanged_command() {
        let prepared = prepare_launch(
            ShellKind::PowerShell,
            "/work",
            None,
            Path::new("claude"),
            &[],
            None,
            Err(std::io::Error::other("no dir")),
            "pty-ps",
        )
        .unwrap();

        assert_eq!(
            prepared.command,
            "Set-Location -LiteralPath '/work'; & 'claude'\r"
        );
        assert_eq!(prepared.prompt_file, None);
    }
}

#[test]
fn ps_quote_doubles_every_typographic_single_quote_and_the_ascii_one() {
    let quoted = ps_quote("it\u{2019}s \u{2018}x\u{2019} \u{201a}y\u{201b} don't");

    assert_eq!(
        quoted,
        "'it\u{2019}\u{2019}s \u{2018}\u{2018}x\u{2019}\u{2019} \u{201a}\u{201a}y\u{201b}\u{201b} don''t'"
    );
}

#[test]
fn ps_maybe_quote_always_quotes_a_text_with_a_typographic_quote() {
    for quote in ['\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'] {
        let quoted = ps_maybe_quote(&format!("a{quote}b"));

        assert_eq!(
            quoted,
            format!("'a{quote}{quote}b'"),
            "U+{:04X}",
            quote as u32
        );
    }
    assert_eq!(ps_maybe_quote("plain"), "plain");
}
