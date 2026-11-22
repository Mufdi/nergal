// API assumed for the Builder (src/lib.rs, Linux-gated, `dir` is the log directory itself,
// i.e. <cache>/nergal, holding nergal.log, nergal.log.1 and nergal.log.lock):
//   try_lock_log(dir: &Path) -> Option<std::fs::File>
//   retry_lock_log(dir: &Path, timeout: Duration) -> Option<std::fs::File>   (polls try_lock_log)
//   rotate_log(dir: &Path)   renames nergal.log -> nergal.log.1 only; never creates nergal.log
use super::{retry_lock_log, rotate_log, try_lock_log};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

#[test]
fn rotate_keeps_the_crashed_runs_output_as_the_previous_generation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("nergal.log"), "panicked at image.rs:103\n").unwrap();

    rotate_log(dir.path());

    assert_eq!(
        read(dir.path(), "nergal.log.1"),
        "panicked at image.rs:103\n"
    );
    assert!(!dir.path().join("nergal.log").exists());
}

#[test]
fn rotate_keeps_only_one_previous_generation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("nergal.log"), "run B").unwrap();
    fs::write(dir.path().join("nergal.log.1"), "run A").unwrap();

    rotate_log(dir.path());

    assert_eq!(read(dir.path(), "nergal.log.1"), "run B");
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in &names {
        if name != "nergal.log.1" {
            assert_ne!(
                fs::read_to_string(dir.path().join(name)).unwrap_or_default(),
                "run A"
            );
        }
    }
}

#[test]
fn rotate_without_a_previous_log_creates_no_generation() {
    let dir = tempfile::tempdir().unwrap();

    rotate_log(dir.path());

    assert!(!dir.path().join("nergal.log.1").exists());
}

#[test]
fn second_launch_is_denied_the_lock_and_leaves_the_live_log_alone() {
    let dir = tempfile::tempdir().unwrap();
    let owner = try_lock_log(dir.path()).expect("first attempt owns the log");
    fs::write(dir.path().join("nergal.log"), "live run").unwrap();

    assert!(try_lock_log(dir.path()).is_none());

    assert_eq!(read(dir.path(), "nergal.log"), "live run");
    assert!(!dir.path().join("nergal.log.1").exists());
    drop(owner);
}

#[test]
fn startup_attempt_never_waits() {
    let dir = tempfile::tempdir().unwrap();
    let _owner = try_lock_log(dir.path()).unwrap();

    let started = Instant::now();
    let attempt = try_lock_log(dir.path());

    assert!(attempt.is_none());
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn relaunched_primary_takes_the_lock_once_the_parent_releases_it() {
    let dir = tempfile::tempdir().unwrap();
    let parent = try_lock_log(dir.path()).unwrap();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        drop(parent);
    });

    let started = Instant::now();
    let acquired = retry_lock_log(dir.path(), Duration::from_secs(3));

    assert!(
        acquired.is_some(),
        "retry must outlast the parent's release"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
    releaser.join().unwrap();
}

#[test]
fn retry_gives_up_at_the_timeout_and_leaves_the_logs_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let _owner = try_lock_log(dir.path()).unwrap();
    fs::write(dir.path().join("nergal.log"), "live run").unwrap();
    fs::write(dir.path().join("nergal.log.1"), "older run").unwrap();

    let started = Instant::now();
    let acquired = retry_lock_log(dir.path(), Duration::from_millis(300));
    let elapsed = started.elapsed();

    assert!(acquired.is_none());
    assert!(elapsed >= Duration::from_millis(300), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
    assert_eq!(read(dir.path(), "nergal.log"), "live run");
    assert_eq!(read(dir.path(), "nergal.log.1"), "older run");
}

#[test]
fn children_do_not_inherit_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let owner = try_lock_log(dir.path()).unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("2")
        .spawn()
        .unwrap();

    drop(owner);
    let reacquired = try_lock_log(dir.path());

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        reacquired.is_some(),
        "a running child must not keep the lock alive"
    );
}
