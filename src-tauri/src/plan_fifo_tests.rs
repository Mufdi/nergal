#![cfg(unix)]

use super::write_plan_decision;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

fn mkfifo(path: &Path) {
    let c = CString::new(path.as_os_str().as_bytes()).unwrap();
    let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
    assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());
}

fn open_reader(path: &Path) -> std::fs::File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .unwrap()
}

/// A non-blocking FIFO reader sees EOF both before any writer connects and
/// after it closes, so EOF only counts once bytes have arrived.
fn read_until_writer_done(mut f: std::fs::File) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    let give_up = Instant::now() + Duration::from_secs(10);
    while Instant::now() < give_up {
        match f.read(&mut buf) {
            Ok(0) if !out.is_empty() => return out,
            Ok(0) => thread::sleep(Duration::from_millis(1)),
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(e) => panic!("reader error: {e}"),
        }
    }
    out
}

fn big_json() -> Vec<u8> {
    let pad = "x".repeat(100_000 - 20);
    let s = format!("{{\"approved\":true,\"message\":\"{pad}\"}}");
    s.into_bytes()
}

#[test]
fn missing_path_is_err_and_creates_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent");
    let res = write_plan_decision(&path, b"{}", Duration::from_secs(1));
    assert!(res.is_err());
    assert!(!path.exists());
}

#[test]
fn regular_file_is_err_and_content_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain");
    std::fs::write(&path, "x").unwrap();
    let res = write_plan_decision(&path, b"{\"approved\":true}", Duration::from_secs(1));
    assert!(res.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
}

#[test]
fn symlink_to_live_fifo_is_err() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("real");
    mkfifo(&fifo);
    let _reader = open_reader(&fifo);
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&fifo, &link).unwrap();
    let res = write_plan_decision(&link, b"{\"approved\":true}", Duration::from_secs(1));
    assert!(res.is_err());
}

#[test]
fn fifo_without_reader_is_err_within_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("noreader");
    mkfifo(&fifo);
    let start = Instant::now();
    let res = write_plan_decision(&fifo, b"{\"approved\":true}", Duration::from_secs(1));
    assert!(res.is_err());
    assert!(
        start.elapsed() < Duration::from_millis(1500),
        "took {:?}",
        start.elapsed()
    );
}

#[test]
fn reader_opened_first_receives_large_payload_whole() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("big");
    mkfifo(&fifo);
    let reader = open_reader(&fifo);
    let handle = thread::spawn(move || read_until_writer_done(reader));
    let payload = big_json();
    write_plan_decision(&fifo, &payload, Duration::from_secs(5)).unwrap();
    let got = handle.join().unwrap();
    assert_eq!(got.len(), payload.len());
    let v: serde_json::Value = serde_json::from_slice(&got).unwrap();
    assert_eq!(v["approved"], true);
}

#[test]
fn reader_that_never_reads_times_out_on_large_payload() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("stalled");
    mkfifo(&fifo);
    let _reader = open_reader(&fifo);
    let start = Instant::now();
    let res = write_plan_decision(&fifo, &big_json(), Duration::from_millis(200));
    assert!(res.is_err());
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "took {:?}",
        start.elapsed()
    );
}

#[test]
fn reader_opening_late_still_receives_payload() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("late");
    mkfifo(&fifo);
    let fifo_for_reader = fifo.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        read_until_writer_done(open_reader(&fifo_for_reader))
    });
    let payload = b"{\"approved\":true}";
    write_plan_decision(&fifo, payload, Duration::from_secs(1)).unwrap();
    assert_eq!(handle.join().unwrap(), payload);
}

#[test]
fn fifo_without_reader_fails_fast_with_long_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("no-reader");
    mkfifo(&fifo);
    let start = Instant::now();
    let res = write_plan_decision(&fifo, b"{\"approved\":true}", Duration::from_secs(5));
    assert!(res.is_err());
    assert!(
        start.elapsed() < Duration::from_millis(1500),
        "took {:?}",
        start.elapsed()
    );
}

use super::fifo_in_ipc_dir;

#[test]
fn fifo_in_ipc_dir_true_when_parent_is_ipc_dir() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("plan-1-abc.fifo");
    assert!(fifo_in_ipc_dir(&fifo, Some(dir.path())));
}

#[test]
fn fifo_in_ipc_dir_false_for_other_dir() {
    let ipc = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let fifo = other.path().join("plan-1-abc.fifo");
    assert!(!fifo_in_ipc_dir(&fifo, Some(ipc.path())));
}

#[test]
fn fifo_in_ipc_dir_false_without_ipc_dir() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("plan-1-abc.fifo");
    assert!(!fifo_in_ipc_dir(&fifo, None));
}

use super::review_decision_path;

#[test]
fn review_decision_path_returns_same_string_when_parent_is_ipc_dir() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("plan-1-abc.fifo");
    let s = fifo.to_str().unwrap();
    assert_eq!(
        review_decision_path(s, Some(dir.path())),
        Some(s.to_string())
    );
}

#[test]
fn review_decision_path_none_for_other_dir() {
    let ipc = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let fifo = other.path().join("plan-1-abc.fifo");
    assert_eq!(
        review_decision_path(fifo.to_str().unwrap(), Some(ipc.path())),
        None
    );
}

#[test]
fn review_decision_path_none_without_ipc_dir() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("plan-1-abc.fifo");
    assert_eq!(review_decision_path(fifo.to_str().unwrap(), None), None);
}
