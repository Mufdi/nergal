//! Dedicated MCP transport: length-framed JSON over a Unix socket.
//!
//! This is a NEW socket (`/tmp/nergal-mcp.sock`), deliberately NOT the hook
//! socket (`hooks/server.rs:202`), which is fire-and-forget (newline-delimited,
//! no response path) and cannot carry MCP request→response. Framing is a
//! 4-byte little-endian length prefix + payload, which — unlike the hook
//! socket's `BufReader::lines()` — tolerates newlines inside JSON-RPC bodies.
//!
//! The framing helpers ([`read_frame`] / [`write_frame`]) are generic over any
//! async reader/writer so they unit-test against in-memory duplex pipes; the
//! Unix specifics (bind, perms, peer uid) live in [`UnixSocketTransport`] so a
//! future Windows named-pipe transport drops in without touching dispatch.

use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

/// Hard ceiling on a single frame. A JSON-RPC tool result for the directory is
/// kilobytes; 16 MiB is a generous bound that still rejects a corrupt/hostile
/// length field before allocating.
pub const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Read one length-framed message. Returns `Ok(None)` on a clean EOF *between*
/// frames (peer closed). A partial frame (EOF mid-payload) is an error.
/// `read_exact` loops internally, so fragmented reads are handled transparently.
pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        // Clean close at a frame boundary — not an error, just end of stream.
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf);
    if len == 0 {
        return Err(invalid("zero-length frame"));
    }
    if len > MAX_FRAME_BYTES {
        return Err(invalid("frame exceeds MAX_FRAME_BYTES"));
    }
    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

/// Write one length-framed message (4-byte LE length + payload) and flush.
pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES as usize {
        return Err(invalid("frame exceeds MAX_FRAME_BYTES"));
    }
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Owner-only Unix-socket transport for the daemon. Holds the bound listener;
/// `accept` yields a connected `UnixStream` plus the peer's uid for the uid
/// boundary check (the only enforced access control — design Decision 2).
#[cfg(unix)]
pub struct UnixSocketTransport {
    listener: UnixListener,
    path: PathBuf,
}

#[cfg(unix)]
impl UnixSocketTransport {
    /// Bind the daemon socket at `path` with mode `0600`, removing any stale
    /// socket first (mirrors `hooks/server.rs:198`).
    pub fn bind(path: &Path) -> io::Result<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Accept one connection, returning the stream and the peer process uid.
    pub async fn accept(&self) -> io::Result<(UnixStream, u32)> {
        let (stream, _addr) = self.listener.accept().await?;
        let uid = peer_uid(&stream)?;
        Ok((stream, uid))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for UnixSocketTransport {
    fn drop(&mut self) {
        // Best-effort cleanup so a restart re-binds cleanly.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Peer credential uid via `SO_PEERCRED`. The uid wall is the real boundary:
/// a different-uid process is rejected outright. No Windows counterpart — the
/// named-pipe transport (windows-ipc) enforces the equivalent via the client
/// SID, and no ungated caller references `peer_uid` on Windows.
#[cfg(unix)]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let cred = stream.peer_cred()?;
    Ok(cred.uid())
}

/// Connect to the daemon socket (shim side).
#[cfg(unix)]
pub async fn connect(path: &Path) -> io::Result<UnixStream> {
    UnixStream::connect(path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn roundtrip_single_frame() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let payload = b"{\"jsonrpc\":\"2.0\"}";
        write_frame(&mut a, payload).await.unwrap();
        let got = read_frame(&mut b).await.unwrap().unwrap();
        assert_eq!(got, payload);
    }

    #[tokio::test]
    async fn fragmented_read_reassembles() {
        // A tiny duplex buffer forces the payload across multiple reads, so
        // read_exact's internal loop is exercised (the partial-read hazard).
        let (mut a, mut b) = tokio::io::duplex(8);
        let payload = vec![7u8; 5000];
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &payload).await.unwrap();
            payload
        });
        let got = read_frame(&mut b).await.unwrap().unwrap();
        let sent = writer.await.unwrap();
        assert_eq!(got, sent);
    }

    #[tokio::test]
    async fn zero_length_frame_is_error() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&0u32.to_le_bytes()).await.unwrap();
        a.flush().await.unwrap();
        let err = read_frame(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn oversized_length_rejected_before_alloc() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&(MAX_FRAME_BYTES + 1).to_le_bytes())
            .await
            .unwrap();
        a.flush().await.unwrap();
        let err = read_frame(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn clean_eof_between_frames_is_none() {
        let (a, mut b) = tokio::io::duplex(64);
        drop(a); // peer closes with no bytes written
        assert!(read_frame(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn partial_frame_eof_is_error() {
        let (mut a, mut b) = tokio::io::duplex(64);
        // Announce 10 bytes, send 3, then close.
        a.write_all(&10u32.to_le_bytes()).await.unwrap();
        a.write_all(&[1, 2, 3]).await.unwrap();
        a.flush().await.unwrap();
        drop(a);
        let err = read_frame(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn write_rejects_oversized_payload() {
        let (mut a, _b) = tokio::io::duplex(64);
        let payload = vec![0u8; (MAX_FRAME_BYTES as usize) + 1];
        let err = write_frame(&mut a, &payload).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
