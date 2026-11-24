use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

const ENXIO_RETRY: Duration = Duration::from_millis(20);
// A FIFO left on disk with no reader must fail fast; the hook opens its read
// end within microseconds of sending the event. 900 ms keeps the 20 ms retry
// sleep from pushing a no-reader failure past 1 s.
const ENXIO_BUDGET: Duration = Duration::from_millis(900);

pub fn fifo_in_ipc_dir(fifo: &Path, ipc_dir: Option<&Path>) -> bool {
    ipc_dir.is_some_and(|dir| fifo.parent() == Some(dir))
}

pub fn review_decision_path(fifo_path: &str, ipc_dir: Option<&Path>) -> Option<String> {
    fifo_in_ipc_dir(Path::new(fifo_path), ipc_dir).then(|| fifo_path.to_string())
}

fn open_fifo_nonblocking(path: &Path, deadline: Instant) -> io::Result<OwnedFd> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let enxio_deadline = deadline.min(Instant::now() + ENXIO_BUDGET);
    loop {
        let flags = libc::O_WRONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let fd = unsafe { libc::open(c_path.as_ptr(), flags) };
        if fd >= 0 {
            return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::ENXIO) if Instant::now() < enxio_deadline => std::thread::sleep(ENXIO_RETRY),
            Some(libc::EINTR) => {}
            _ => return Err(err),
        }
    }
}

fn ensure_fifo(fd: &OwnedFd) -> io::Result<()> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if st.st_mode & libc::S_IFMT != libc::S_IFIFO {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "decision path is not a FIFO",
        ));
    }
    Ok(())
}

fn wait_writable(fd: &OwnedFd, deadline: Instant) -> io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "timed out writing plan decision",
        ));
    }
    let timeout_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
    let mut pfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    if rc < 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            return Ok(());
        }
        return Err(err);
    }
    if rc > 0 && pfd.revents & libc::POLLERR != 0 {
        return Err(io::Error::from_raw_os_error(libc::EPIPE));
    }
    Ok(())
}

/// Delivers a plan decision to the hook's FIFO without ever blocking past
/// `deadline`. Never creates the path: a missing FIFO means the hook is gone.
pub fn write_plan_decision(path: &Path, payload: &[u8], deadline: Duration) -> io::Result<()> {
    let deadline = Instant::now() + deadline;
    let fd = open_fifo_nonblocking(path, deadline)?;
    ensure_fifo(&fd)?;

    let mut written = 0;
    while written < payload.len() {
        let rest = &payload[written..];
        let n = unsafe { libc::write(fd.as_raw_fd(), rest.as_ptr().cast(), rest.len()) };
        if n >= 0 {
            written += n as usize;
            continue;
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) => wait_writable(&fd, deadline)?,
            Some(libc::EINTR) => {}
            _ => return Err(err),
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "plan_fifo_tests.rs"]
mod tests;
