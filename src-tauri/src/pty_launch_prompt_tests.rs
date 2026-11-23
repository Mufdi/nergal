use super::*;
use crate::agents::state::AgentRuntimeState;
#[cfg(unix)]
use crate::agents::write_launch_prompt_file;
use crate::agents::{LaunchPrompt, PromptAttach, clear_launch_prompts_in, launch_prompt_file_path};
use crate::config::ShellKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SPEC_PROMPT: &str =
    "line one\nline two <!-- x --> !! and !# here\nline three: `echo hi` $(echo no) it's done";

fn agents() -> AgentRuntimeState {
    AgentRuntimeState::bootstrap().unwrap()
}

fn is_live(agents: &AgentRuntimeState, session_id: &str) -> bool {
    agents.live_session_ids().contains(session_id)
}

fn prompt(text: &str) -> LaunchPrompt {
    LaunchPrompt {
        text: text.to_string(),
        attach: PromptAttach::AfterDoubleDash,
    }
}

fn dir_is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).unwrap().next().is_none()
}

#[cfg(unix)]
mod real_shell {
    use super::*;
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};

    struct Harness {
        writer: Box<dyn Write + Send>,
        output: Arc<Mutex<Vec<u8>>>,
        child: Box<dyn portable_pty::Child + Send + Sync>,
        _master: Box<dyn portable_pty::MasterPty + Send>,
        tmp: tempfile::TempDir,
        agent: PathBuf,
        out: PathBuf,
        done: PathBuf,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = self.child.kill();
        }
    }

    fn on_path(bin: &str) -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
            .unwrap_or(false)
    }

    fn wait_for(deadline: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + deadline;
        while Instant::now() < end {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    // Shell is started with a marker PS1 so the test only types once readline
    // (and the magic-space binding) is live; input typed earlier would bypass it.
    fn start_shell(shell: &str) -> Option<Harness> {
        if !on_path(shell) {
            eprintln!("skipping: {shell} not on PATH");
            return None;
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let out = tmp.path().join("OUT");
        let done = tmp.path().join("DONE");
        let agent = tmp.path().join("fake-agent");
        std::fs::write(
            &agent,
            format!(
                "#!/bin/sh\nfor a; do last=$a; done\nprintf '%s' \"$last\" > '{}'\n: > '{}'\n",
                out.display(),
                done.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("HOME", &home);
        cmd.env("TERM", "xterm");
        match shell {
            "bash" => {
                let rc = tmp.path().join("rc");
                std::fs::write(
                    &rc,
                    "bind Space:magic-space\nHISTFILE=/dev/null\nPS1='READY> '\n",
                )
                .unwrap();
                cmd.args(["--noprofile", "--rcfile"]);
                cmd.arg(&rc);
                cmd.arg("-i");
            }
            _ => {
                let zdot = tmp.path().join("zdot");
                std::fs::create_dir_all(&zdot).unwrap();
                std::fs::write(
                    zdot.join(".zshrc"),
                    "bindkey ' ' magic-space\nHISTFILE=/dev/null\nPROMPT='READY> '\n",
                )
                .unwrap();
                cmd.env("ZDOTDIR", &zdot);
                cmd.arg("-i");
            }
        }
        cmd.cwd(tmp.path());

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 200,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });

        let h = Harness {
            writer,
            output,
            child,
            _master: pair.master,
            tmp,
            agent,
            out,
            done,
        };
        assert!(
            wait_for(Duration::from_secs(10), || h
                .output_text()
                .contains("READY>")),
            "shell never showed its prompt: {}",
            h.output_text()
        );
        Some(h)
    }

    impl Harness {
        fn output_text(&self) -> String {
            String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
        }

        fn send(&mut self, command: &str) {
            self.writer.write_all(command.as_bytes()).unwrap();
            self.writer.flush().unwrap();
        }

        fn launch(&mut self, prompt_ref: &PromptRef) -> String {
            let cwd = self.tmp.path().display().to_string();
            let cmd = build_launch_command(
                ShellKind::Posix,
                &cwd,
                None,
                &self.agent,
                &[],
                Some((prompt_ref, &PromptAttach::AfterDoubleDash)),
            );
            self.send(&cmd);
            cmd
        }

        fn delivered(&self) -> Option<String> {
            self.done
                .exists()
                .then(|| std::fs::read_to_string(&self.out).unwrap())
        }
    }

    fn prompt_dir(h: &Harness) -> PathBuf {
        let dir = h.tmp.path().join("prompts");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_form_delivers_byte_identical(shell: &str) {
        let Some(mut h) = start_shell(shell) else {
            return;
        };
        let file = write_launch_prompt_file(&prompt_dir(&h), "pty-test-1", SPEC_PROMPT).unwrap();

        h.launch(&PromptRef::File(file.clone()));

        assert!(
            wait_for(Duration::from_secs(10), || h.done.exists()),
            "agent never started: {}",
            h.output_text()
        );
        assert_eq!(h.delivered().as_deref(), Some(SPEC_PROMPT));
        assert!(
            !file.exists(),
            "the shell must delete the prompt file as it reads it"
        );
        let shown = h.output_text();
        assert!(!shown.contains("parse error"), "{shown}");
        assert!(!shown.contains("event not found"), "{shown}");
    }

    #[test]
    fn bash_with_magic_space_delivers_the_file_prompt_byte_identical() {
        file_form_delivers_byte_identical("bash");
    }

    #[test]
    fn zsh_with_magic_space_delivers_the_file_prompt_byte_identical() {
        file_form_delivers_byte_identical("zsh");
    }

    #[test]
    fn the_inline_prompt_is_corrupted_by_history_expansion_which_the_file_form_avoids() {
        let Some(mut h) = start_shell("bash") else {
            return;
        };

        h.launch(&PromptRef::Inline(SPEC_PROMPT.to_string()));
        wait_for(Duration::from_secs(3), || h.done.exists());

        assert_ne!(h.delivered().as_deref(), Some(SPEC_PROMPT));
    }

    #[test]
    fn replaying_the_command_after_the_file_is_gone_does_not_start_the_agent() {
        let Some(mut h) = start_shell("bash") else {
            return;
        };
        let file = write_launch_prompt_file(&prompt_dir(&h), "pty-test-2", "plan").unwrap();
        let cmd = h.launch(&PromptRef::File(file));
        assert!(wait_for(Duration::from_secs(10), || h.done.exists()));
        std::fs::remove_file(&h.done).unwrap();

        h.send(&cmd);
        let after = h.tmp.path().join("AFTER");
        h.send(&format!(" : > '{}'\n", after.display()));

        assert!(
            wait_for(Duration::from_secs(10), || after.exists()),
            "shell stopped responding: {}",
            h.output_text()
        );
        assert!(!h.done.exists(), "the replay started the agent");
    }

    #[test]
    fn the_prompt_file_is_private_and_holds_exactly_the_prompt() {
        let dir = tempfile::tempdir().unwrap();

        let file = write_launch_prompt_file(dir.path(), "pty-mode", "plan").unwrap();

        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "plan");
        let again = write_launch_prompt_file(dir.path(), "pty-mode", "plan2").unwrap();
        assert_eq!(std::fs::read_to_string(again).unwrap(), "plan2");
    }
}

#[cfg(unix)]
mod command_shape {
    use super::*;

    fn file_ref(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = write_launch_prompt_file(dir.path(), "pty-shape", text).unwrap();
        (dir, path)
    }

    #[test]
    fn the_typed_command_never_holds_the_prompt_text_and_has_one_trailing_newline() {
        let (_dir, path) = file_ref("hello 'world' !# $(id)");

        let cmd = build_launch_command(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("claude"),
            &[],
            Some((
                &PromptRef::File(path.clone()),
                &PromptAttach::AfterDoubleDash,
            )),
        );

        for leaked in ["hello", "!#", "$(id)"] {
            assert!(!cmd.contains(leaked), "{leaked} leaked into {cmd}");
        }
        assert!(cmd.contains(&format!("'{}'", path.display())), "{cmd}");
        assert_eq!(cmd.matches('\n').count(), 1);
        assert!(cmd.ends_with('\n'));
    }

    #[test]
    fn after_double_dash_puts_the_reference_last_right_behind_the_separator() {
        let (_dir, path) = file_ref("--settings={}");

        let cmd = build_launch_command(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("claude"),
            &["--continue".to_string()],
            Some((
                &PromptRef::File(path.clone()),
                &PromptAttach::AfterDoubleDash,
            )),
        );

        let p = path.display();
        let expected_tail =
            format!("claude --continue -- \"$(command cat -- '{p}'; command rm -f -- '{p}')\"\n");
        assert!(cmd.ends_with(&expected_tail), "{cmd}");
    }

    #[test]
    fn a_flag_attach_glues_the_reference_to_the_flag() {
        let (_dir, path) = file_ref("--help");

        let cmd = build_launch_command(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("opencode"),
            &[],
            Some((
                &PromptRef::File(path.clone()),
                &PromptAttach::Flag("--prompt"),
            )),
        );

        let (_, after_flag) = cmd.split_once("--prompt=").unwrap();
        assert!(
            after_flag.starts_with(|c: char| !c.is_whitespace()),
            "{cmd}"
        );
        assert!(after_flag.contains(&path.display().to_string()), "{cmd}");
        assert!(!cmd.contains("--prompt "), "{cmd}");
    }

    #[test]
    fn a_readability_test_on_the_file_precedes_the_binary() {
        let (_dir, path) = file_ref("plan");

        let cmd = build_launch_command(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("claude"),
            &[],
            Some((
                &PromptRef::File(path.clone()),
                &PromptAttach::AfterDoubleDash,
            )),
        );

        assert!(
            cmd.contains(&format!("test -r '{}' && claude", path.display())),
            "{cmd}"
        );
    }

    #[test]
    fn prepare_launch_delivers_the_normalized_prompt_through_a_file() {
        let dir = tempfile::tempdir().unwrap();

        let prepared = prepare_launch(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("claude"),
            &[],
            Some(&prompt("a\0b\n\n")),
            Ok(dir.path().to_path_buf()),
            "pty-prep",
        )
        .unwrap();

        let file = prepared.prompt_file.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "ab");
        assert!(prepared.command.contains(&file.display().to_string()));
    }

    #[test]
    fn prepare_launch_without_a_usable_prompt_adds_nothing_and_creates_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let args = ["--continue".to_string()];
        let expected = " cd '/work' && claude --continue\n";
        let blank = prompt("\n\n");
        let cases: [(&str, Option<&LaunchPrompt>, std::io::Result<PathBuf>); 2] = [
            ("only newlines", Some(&blank), Ok(dir.path().to_path_buf())),
            (
                "absent with an unusable dir",
                None,
                Err(std::io::Error::other("no dir")),
            ),
        ];

        for (name, prompt, launch_dir) in cases {
            let prepared = prepare_launch(
                ShellKind::Posix,
                "/work",
                None,
                Path::new("claude"),
                &args,
                prompt,
                launch_dir,
                "pty-none",
            )
            .unwrap();

            assert_eq!(prepared.command, expected, "{name}");
            assert_eq!(prepared.prompt_file, None, "{name}");
        }
        assert!(dir_is_empty(dir.path()));
    }

    #[test]
    fn prepare_launch_fails_and_leaves_no_file_when_the_dir_cannot_be_created() {
        let tmp = tempfile::tempdir().unwrap();
        let blocker = tmp.path().join("file");
        std::fs::write(&blocker, "x").unwrap();

        let result = prepare_launch(
            ShellKind::Posix,
            "/work",
            None,
            Path::new("claude"),
            &[],
            Some(&prompt("hello")),
            Ok(blocker.join("launch-prompts")),
            "pty-fail",
        );

        assert!(result.is_err());
        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries, vec![blocker]);
    }

    #[test]
    fn a_launch_prompt_file_path_rejects_stems_that_could_escape_or_break_quoting() {
        let dir = Path::new("/run/nergal/launch-prompts");

        for bad in ["a/b", "..", "a'b", "a b", ""] {
            let err = launch_prompt_file_path(dir, bad).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
        }
        assert_eq!(
            launch_prompt_file_path(dir, "pty-1").unwrap(),
            dir.join("pty-1.txt")
        );
    }
}

#[test]
fn normalization_strips_nul_and_trailing_newlines_and_drops_an_empty_prompt() {
    let cases: [(&str, Option<&str>); 3] = [("a\0b\n\n", Some("ab")), ("\n\n", None), ("", None)];

    for (input, expected) in cases {
        assert_eq!(
            normalize_launch_prompt(input).as_deref(),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn send_launch_writes_exactly_the_command() {
    let prepared = PreparedLaunch {
        command: " cd '/w' && claude\n".to_string(),
        prompt_file: None,
    };
    let mut sink: Vec<u8> = Vec::new();

    send_launch(&prepared, Some(&mut sink as &mut dyn Write)).unwrap();

    assert_eq!(sink, prepared.command.as_bytes());
}

#[test]
fn send_launch_fails_without_a_writer_or_when_the_write_errors() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("pty closed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("pty closed"))
        }
    }
    let prepared = PreparedLaunch {
        command: "x\n".to_string(),
        prompt_file: None,
    };

    assert!(send_launch(&prepared, None).is_err());
    assert!(send_launch(&prepared, Some(&mut Broken as &mut dyn Write)).is_err());
}

fn armed_launch<'a>(
    mgr: &'a PtyManager,
    agents: &'a AgentRuntimeState,
    session_id: &'a str,
    pty_id: &'a str,
) -> LaunchRollback<'a> {
    assert_eq!(
        reserve_session_pty(mgr, agents, session_id, pty_id).unwrap(),
        None
    );
    LaunchRollback::new(mgr, agents, session_id, pty_id)
}

fn queued_prompt(mgr: &PtyManager, session_id: &str) -> Option<String> {
    mgr.pending_prompts.lock().unwrap().get(session_id).cloned()
}

#[cfg(unix)]
mod rollback {
    use super::*;

    #[test]
    fn a_dropped_armed_guard_leaves_no_trace_and_requeues_the_prompt() {
        let (mgr, agents) = (PtyManager::new(), agents());
        let dir = tempfile::tempdir().unwrap();
        let file = write_launch_prompt_file(dir.path(), "pty-rb-1", "hello").unwrap();
        {
            let mut guard = armed_launch(&mgr, &agents, "lp-rb-1", "pty-rb-1");
            guard.set_prompt(Some("hello".to_string()));
            guard.set_file(file.clone());
        }

        assert!(mgr.session_ptys.lock().unwrap().get("lp-rb-1").is_none());
        assert!(!is_live(&agents, "lp-rb-1"));
        assert_eq!(queued_prompt(&mgr, "lp-rb-1").as_deref(), Some("hello"));
        assert!(!file.exists());
        assert!(!take_launched(&mgr, "pty-rb-1"));
    }

    #[test]
    fn a_newer_queued_prompt_survives_the_rollback() {
        let (mgr, agents) = (PtyManager::new(), agents());
        {
            let mut guard = armed_launch(&mgr, &agents, "lp-rb-2", "pty-rb-2");
            guard.set_prompt(Some("old".to_string()));
            mgr.pending_prompts
                .lock()
                .unwrap()
                .insert("lp-rb-2".to_string(), "new".to_string());
        }

        assert_eq!(queued_prompt(&mgr, "lp-rb-2").as_deref(), Some("new"));
    }

    #[test]
    fn a_disarmed_guard_changes_nothing_and_marks_the_pty_launched_once() {
        let (mgr, agents) = (PtyManager::new(), agents());
        let dir = tempfile::tempdir().unwrap();
        let file = write_launch_prompt_file(dir.path(), "pty-rb-3", "hello").unwrap();
        {
            let mut guard = armed_launch(&mgr, &agents, "lp-rb-3", "pty-rb-3");
            guard.set_prompt(Some("hello".to_string()));
            guard.set_file(file.clone());
            guard.disarm();
        }

        assert_eq!(
            mgr.session_ptys.lock().unwrap().get("lp-rb-3"),
            Some(&"pty-rb-3".to_string())
        );
        assert!(is_live(&agents, "lp-rb-3"));
        assert_eq!(queued_prompt(&mgr, "lp-rb-3"), None);
        assert!(file.exists());
        assert!(take_launched(&mgr, "pty-rb-3"));
        assert!(!take_launched(&mgr, "pty-rb-3"));
    }

    #[test]
    fn a_rollback_of_a_launch_that_lost_its_reservation_leaves_the_new_owner_alone() {
        let (mgr, agents) = (PtyManager::new(), agents());
        reserve_session_pty(&mgr, &agents, "lp-rb-4", "pty-owner").unwrap();
        {
            let mut guard = LaunchRollback::new(&mgr, &agents, "lp-rb-4", "pty-gone");
            guard.set_prompt(Some("hello".to_string()));
        }

        assert_eq!(
            mgr.session_ptys.lock().unwrap().get("lp-rb-4"),
            Some(&"pty-owner".to_string())
        );
        assert!(is_live(&agents, "lp-rb-4"));
        assert!(!take_launched(&mgr, "pty-gone"));
    }

    #[test]
    fn a_rolled_back_launch_frees_the_reservation_for_a_retry() {
        let (mgr, agents) = (PtyManager::new(), agents());
        drop(armed_launch(&mgr, &agents, "lp-rb-5", "pty-rb-5a"));

        let retry = reserve_session_pty(&mgr, &agents, "lp-rb-5", "pty-rb-5b").unwrap();

        assert_eq!(retry, None);
    }

    #[test]
    fn discarding_removes_only_the_ptys_prompt_file_and_tolerates_missing_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let mine = write_launch_prompt_file(dir.path(), "pty-d1", "x").unwrap();
        let other = write_launch_prompt_file(dir.path(), "pty-d2", "y").unwrap();

        discard_launch_prompt(Ok(dir.path().to_path_buf()), "pty-d1");
        discard_launch_prompt(Ok(dir.path().to_path_buf()), "pty-d1");
        discard_launch_prompt(Err(std::io::Error::other("no dir")), "pty-d2");

        assert!(!mine.exists());
        assert!(other.exists());
    }

    #[test]
    fn the_startup_sweep_empties_the_prompt_dir_and_ignores_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        write_launch_prompt_file(dir.path(), "pty-s1", "x").unwrap();
        write_launch_prompt_file(dir.path(), "pty-s2", "y").unwrap();

        clear_launch_prompts_in(dir.path());
        clear_launch_prompts_in(&dir.path().join("missing"));

        assert!(dir_is_empty(dir.path()));
    }
}
