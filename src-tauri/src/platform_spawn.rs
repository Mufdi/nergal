//! Cross-platform subprocess helpers.
//!
//! On Windows a GUI app has no attached console, so every background (non-PTY)
//! child spawned without `CREATE_NO_WINDOW` allocates a fresh console window
//! that flashes on screen — periodic pollers (git status, agent health checks)
//! strobe a `cmd` window every few seconds. POSIX has no analog; the helpers are
//! no-ops off Windows. PTY children are exempt: ConPTY owns their console.

/// `CREATE_NO_WINDOW` (winbase.h) — run the child without allocating a console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Suppress the console window of a background child process on Windows.
/// Chainable mid-builder: `Command::new("git").no_window().args(..)`.
pub trait NoWindow {
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindow for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl NoWindow for tokio::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

/// Spawn a fire-and-forget child and reap it on a detached thread. Dropping a
/// `Child` without `wait()` leaves a zombie per spawn on unix for the app's
/// whole lifetime (BUG-40).
pub trait SpawnReaped {
    fn spawn_reaped(&mut self) -> std::io::Result<u32>;
}

impl SpawnReaped for std::process::Command {
    fn spawn_reaped(&mut self) -> std::io::Result<u32> {
        let mut child = self.spawn()?;
        let pid = child.id();
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(pid)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::SpawnReaped;
    use std::time::{Duration, Instant};

    #[test]
    fn spawn_reaped_leaves_no_zombie() {
        let pid = std::process::Command::new("true").spawn_reaped().unwrap() as libc::pid_t;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut status = 0;
            // SAFETY: plain waitpid probe on a pid this process spawned.
            let r = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            let reaped =
                r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
            if reaped {
                return;
            }
            assert!(Instant::now() < deadline, "child {pid} was never reaped");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
