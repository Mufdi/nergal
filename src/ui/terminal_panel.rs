use std::cell::Cell;
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gpui::*;
use gpui_ghostty_terminal::view::{TerminalInput, TerminalView};
use gpui_ghostty_terminal::{TerminalConfig, TerminalSession};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::ui::size_observer::{CellMetrics, TerminalSizeElement};

pub struct TerminalPanel {
    terminal_view: Entity<TerminalView>,
    focus_handle: FocusHandle,
    stdin_tx: mpsc::Sender<Vec<u8>>,
    pty_master: Arc<dyn portable_pty::MasterPty + Send>,
    last_panel_size: Rc<Cell<Size<Pixels>>>,
    cell_metrics: Rc<Option<CellMetrics>>,
}

impl TerminalPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = TerminalConfig {
            update_window_title: false,
            ..Default::default()
        };

        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty failed");

        let master: Arc<dyn portable_pty::MasterPty + Send> = Arc::from(pty_pair.master);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "cluihud");

        let mut child = pty_pair
            .slave
            .spawn_command(cmd)
            .expect("spawn login shell failed");

        thread::spawn(move || {
            let _ = child.wait();
        });

        let mut pty_reader = master.try_clone_reader().expect("pty reader");
        let mut pty_writer = master.take_writer().expect("pty writer");

        let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>();

        thread::spawn(move || {
            while let Ok(bytes) = stdin_rx.recv() {
                if pty_writer.write_all(&bytes).is_err() {
                    break;
                }
                let _ = pty_writer.flush();
            }
        });

        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let n = match pty_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                let _ = stdout_tx.send(buf[..n].to_vec());
            }
        });

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);

        let terminal_focus = focus_handle.clone();
        let terminal_view = cx.new(|_cx| {
            let session = TerminalSession::new(config).expect("vt init");
            let tx = stdin_tx.clone();
            let input = TerminalInput::new(move |bytes| {
                let _ = tx.send(bytes.to_vec());
            });

            TerminalView::new_with_input(session, terminal_focus, input)
        });

        let cell_metrics = Rc::new(Self::compute_cell_metrics(window));

        // Stdout polling — only handles output, no resize
        let view_for_task = terminal_view.clone();
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;

                    let mut batch = Vec::new();
                    while let Ok(chunk) = stdout_rx.try_recv() {
                        batch.extend_from_slice(&chunk);
                    }
                    if batch.is_empty() {
                        continue;
                    }

                    cx.update(|_, cx| {
                        view_for_task.update(cx, |this, cx| {
                            this.queue_output_bytes(&batch, cx);
                        });
                    })
                    .ok();
                }
            })
            .detach();

        Self {
            terminal_view,
            focus_handle,
            stdin_tx,
            pty_master: master,
            last_panel_size: Rc::new(Cell::new(Size::default())),
            cell_metrics,
        }
    }

    /// Computes terminal cell dimensions from font metrics (once at init).
    fn compute_cell_metrics(window: &mut Window) -> Option<CellMetrics> {
        let mut style = window.text_style();
        let font = gpui_ghostty_terminal::default_terminal_font();
        style.font_family = font.family.clone();
        style.font_features = gpui_ghostty_terminal::default_terminal_font_features();
        style.font_fallbacks = font.fallbacks.clone();

        let rem_size = window.rem_size();
        let font_size = style.font_size.to_pixels(rem_size);
        let line_height = style.line_height.to_pixels(style.font_size, rem_size);

        let run = style.to_run(1);
        let lines = window
            .text_system()
            .shape_text(SharedString::from("M"), font_size, &[run], None, Some(1))
            .ok()?;
        let line = lines.first()?;

        Some(CellMetrics {
            width: f32::from(line.width()).max(1.0),
            height: f32::from(line_height).max(1.0),
        })
    }

    /// Focus this panel's terminal.
    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    /// Writes raw bytes to the PTY stdin (e.g. "y\n" for accept).
    pub fn write_to_pty(&self, bytes: &[u8]) {
        let _ = self.stdin_tx.send(bytes.to_vec());
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(TerminalSizeElement::new(
                self.terminal_view.clone(),
                self.last_panel_size.clone(),
                self.pty_master.clone(),
                self.terminal_view.clone(),
                self.cell_metrics.clone(),
            ))
    }
}
