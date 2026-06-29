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
