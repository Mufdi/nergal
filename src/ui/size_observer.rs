use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, Entity, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Size, Style, Window,
};
use gpui_ghostty_terminal::view::TerminalView;
use portable_pty::PtySize;

/// Cached cell dimensions for cols/rows calculation.
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

/// Element wrapper that performs synchronous PTY + VT resize in prepaint.
///
/// Like Zed's terminal element, the resize happens in the same frame
/// as layout — before paint — so the grid is always consistent with
/// the rendered output. This avoids cursor drift from async resize.
pub struct TerminalSizeElement {
    child: Option<AnyElement>,
    last_size: Rc<Cell<Size<Pixels>>>,
    pty_master: Arc<dyn portable_pty::MasterPty + Send>,
    terminal_view: Entity<TerminalView>,
    cell_metrics: Rc<Option<CellMetrics>>,
}

impl TerminalSizeElement {
    pub fn new(
        child: impl IntoElement,
        last_size: Rc<Cell<Size<Pixels>>>,
        pty_master: Arc<dyn portable_pty::MasterPty + Send>,
        terminal_view: Entity<TerminalView>,
        cell_metrics: Rc<Option<CellMetrics>>,
    ) -> Self {
        Self {
            child: Some(child.into_any_element()),
            last_size,
            pty_master,
            terminal_view,
            cell_metrics,
        }
    }
}

impl IntoElement for TerminalSizeElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalSizeElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = self.child.take().expect("child already taken");
        let child_layout_id = child.request_layout(window, cx);

        let style = Style {
            size: Size::full(),
            flex_grow: 1.0,
            ..Style::default()
        };

        let layout_id = window.request_layout(style, [child_layout_id], cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let current = bounds.size;
        let prev = self.last_size.get();

        let width = f32::from(current.width);
        let height = f32::from(current.height);

        if current != prev && width > 0.0 && height > 0.0 {
            if let Some(ref metrics) = *self.cell_metrics {
                let cols = (width / metrics.width).floor().max(1.0) as u16;
                let rows = (height / metrics.height).floor().max(1.0) as u16;

                // VT grid resize first — reflow + immediate viewport refresh
                self.terminal_view.update(cx, |tv, cx| {
                    tv.resize_terminal(cols, rows, cx);
                });

                // PTY resize second — SIGWINCH reaches shell after VT grid is ready
                let _ = self.pty_master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }

            self.last_size.set(current);
        }

        child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
    }
}
