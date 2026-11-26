use super::*;
use std::io::{ErrorKind, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Accepts the first write and fails every later one.
struct FailsAfterFirst {
    seen: Arc<Mutex<Vec<u8>>>,
    calls: usize,
}

impl Write for FailsAfterFirst {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.calls += 1;
        if self.calls > 1 {
            return Err(std::io::Error::other("pty gone"));
        }
        self.seen.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn wait_for_len(sink: &Sink, len: usize) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let bytes = sink.0.lock().unwrap().clone();
        if bytes.len() >= len || Instant::now() >= deadline {
            return bytes;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn bytes_from_two_writer_clones_reach_the_inner_writer_in_send_order() {
    let sink = Sink::default();
    let queue = PtyWriteQueue::spawn(Box::new(sink.clone()));
    let mut first = queue.writer();
    let mut second = first.clone();

    first.write_all(b"one-").unwrap();
    second.write_all(b"two-").unwrap();
    first.write_all(b"three-").unwrap();
    second.write_all(b"four").unwrap();

    assert_eq!(wait_for_len(&sink, 19), b"one-two-three-four");
}

#[test]
fn the_queue_thread_stops_after_an_inner_write_error() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let queue = PtyWriteQueue::spawn(Box::new(FailsAfterFirst {
        seen: Arc::clone(&seen),
        calls: 0,
    }));
    let mut writer = queue.writer();
    writer.write_all(b"ok").unwrap();
    // The thread may already have stopped by the time the later writes are sent.
    let _ = writer.write_all(b"fails");
    let _ = writer.write_all(b"never-written");

    // A closed channel is how the stopped thread becomes observable.
    let deadline = Instant::now() + Duration::from_secs(2);
    let closed = loop {
        match writer.write(b"x") {
            Err(e) => break e,
            Ok(_) if Instant::now() >= deadline => panic!("queue thread never stopped"),
            Ok(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    };

    assert_eq!(closed.kind(), ErrorKind::BrokenPipe);
    assert_eq!(*seen.lock().unwrap(), b"ok");
}
