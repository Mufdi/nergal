use std::io::{self, Write};
use std::sync::mpsc::{Sender, channel};

pub struct PtyWriteQueue {
    tx: Sender<Vec<u8>>,
}

#[derive(Clone)]
pub struct QueueWriter {
    tx: Sender<Vec<u8>>,
}

impl PtyWriteQueue {
    pub fn spawn(mut inner: Box<dyn Write + Send>) -> PtyWriteQueue {
        let (tx, rx) = channel::<Vec<u8>>();
        let spawned = std::thread::Builder::new()
            .name("pty-write-queue".into())
            .spawn(move || {
                while let Ok(bytes) = rx.recv() {
                    if let Err(e) = inner.write_all(&bytes).and_then(|()| inner.flush()) {
                        tracing::warn!("pty write queue stopped: {e}");
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("pty write queue thread failed to start: {e}");
        }
        PtyWriteQueue { tx }
    }

    pub fn writer(&self) -> QueueWriter {
        QueueWriter {
            tx: self.tx.clone(),
        }
    }
}

impl Write for QueueWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "write_queue_tests.rs"]
mod tests;
