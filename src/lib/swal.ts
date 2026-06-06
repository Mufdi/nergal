import Swal, { type SweetAlertOptions, type SweetAlertResult } from "sweetalert2";

/**
 * Pixel-art SVG icons for sweetalert2 modals. Each renders as an 8-bit
 * blocky glyph drawn on a 12x12 grid with `shape-rendering: crispEdges`
 * so the pixel quality survives when scaled up. Colors track theme tokens
 * so the icons read on any background.
 */
const PIXEL_WARNING = `
<svg viewBox="0 0 12 12" shape-rendering="crispEdges" width="56" height="56" aria-hidden="true">
  <rect x="5" y="0" width="2" height="1" fill="#facc15"/>
  <rect x="4" y="1" width="4" height="2" fill="#facc15"/>
  <rect x="3" y="3" width="6" height="2" fill="#facc15"/>
  <rect x="2" y="5" width="8" height="2" fill="#facc15"/>
  <rect x="1" y="7" width="10" height="2" fill="#facc15"/>
  <rect x="0" y="9" width="12" height="3" fill="#facc15"/>
  <rect x="5" y="4" width="2" height="4" fill="#1a1a1a"/>
  <rect x="5" y="9" width="2" height="1" fill="#1a1a1a"/>
</svg>`.trim();

const PIXEL_ERROR = `
<svg viewBox="0 0 12 12" shape-rendering="crispEdges" width="56" height="56" aria-hidden="true">
  <rect x="1" y="1" width="10" height="10" fill="#dc2626"/>
  <rect x="2" y="2" width="2" height="2" fill="#ffffff"/>
  <rect x="8" y="2" width="2" height="2" fill="#ffffff"/>
  <rect x="3" y="3" width="2" height="2" fill="#ffffff"/>
  <rect x="7" y="3" width="2" height="2" fill="#ffffff"/>
  <rect x="4" y="4" width="2" height="2" fill="#ffffff"/>
  <rect x="6" y="4" width="2" height="2" fill="#ffffff"/>
  <rect x="5" y="5" width="2" height="2" fill="#ffffff"/>
  <rect x="4" y="6" width="2" height="2" fill="#ffffff"/>
  <rect x="6" y="6" width="2" height="2" fill="#ffffff"/>
  <rect x="3" y="7" width="2" height="2" fill="#ffffff"/>
  <rect x="7" y="7" width="2" height="2" fill="#ffffff"/>
  <rect x="2" y="8" width="2" height="2" fill="#ffffff"/>
  <rect x="8" y="8" width="2" height="2" fill="#ffffff"/>
</svg>`.trim();

const PIXEL_SUCCESS = `
<svg viewBox="0 0 12 12" shape-rendering="crispEdges" width="56" height="56" aria-hidden="true">
  <rect x="1" y="1" width="10" height="10" fill="#16a34a"/>
  <rect x="9" y="3" width="2" height="1" fill="#ffffff"/>
  <rect x="8" y="4" width="2" height="1" fill="#ffffff"/>
  <rect x="7" y="5" width="2" height="1" fill="#ffffff"/>
  <rect x="6" y="6" width="2" height="1" fill="#ffffff"/>
  <rect x="5" y="7" width="2" height="1" fill="#ffffff"/>
  <rect x="4" y="7" width="2" height="1" fill="#ffffff"/>
  <rect x="3" y="6" width="2" height="1" fill="#ffffff"/>
  <rect x="2" y="5" width="2" height="1" fill="#ffffff"/>
</svg>`.trim();

const PIXEL_INFO = `
<svg viewBox="0 0 12 12" shape-rendering="crispEdges" width="56" height="56" aria-hidden="true">
  <rect x="1" y="1" width="10" height="10" fill="#2563eb"/>
  <rect x="5" y="2" width="2" height="2" fill="#ffffff"/>
  <rect x="5" y="5" width="2" height="5" fill="#ffffff"/>
</svg>`.trim();

const PIXEL_QUESTION = `
<svg viewBox="0 0 12 12" shape-rendering="crispEdges" width="56" height="56" aria-hidden="true">
  <rect x="1" y="1" width="10" height="10" fill="#6366f1"/>
  <rect x="4" y="3" width="4" height="1" fill="#ffffff"/>
  <rect x="3" y="3" width="1" height="2" fill="#ffffff"/>
  <rect x="8" y="3" width="1" height="3" fill="#ffffff"/>
  <rect x="6" y="6" width="2" height="1" fill="#ffffff"/>
  <rect x="5" y="6" width="2" height="2" fill="#ffffff"/>
  <rect x="5" y="9" width="2" height="1" fill="#ffffff"/>
</svg>`.trim();

type CluihudSwalKind = "warning" | "error" | "success" | "info" | "question";

const ICON_HTML: Record<CluihudSwalKind, string> = {
  warning: PIXEL_WARNING,
  error: PIXEL_ERROR,
  success: PIXEL_SUCCESS,
  info: PIXEL_INFO,
  question: PIXEL_QUESTION,
};

/**
 * Theme-matched defaults for every cluihud Swal. We disable the library's
 * default icon set, inject a pixel-art SVG via `iconHtml`, and let the
 * `customClass` overrides in `globals.css` do the surface/typography work.
 */
function baseOptions(kind: CluihudSwalKind, opts: SweetAlertOptions): SweetAlertOptions {
  return {
    icon: undefined,
    iconHtml: ICON_HTML[kind],
    buttonsStyling: false,
    // SweetAlert2 default forces `html/body { height: auto !important }`, which
    // breaks our `h-full` cascade from `#root` and shrinks the main column,
    // raising StatusBar and leaving a void below while the modal is open.
    heightAuto: false,
    showClass: { popup: "" },
    hideClass: { popup: "" },
    customClass: {
      popup: "cluihud-swal-popup",
      title: "cluihud-swal-title",
      htmlContainer: "cluihud-swal-body",
      actions: "cluihud-swal-actions",
      icon: "cluihud-swal-icon",
      confirmButton: "cluihud-swal-confirm",
      cancelButton: "cluihud-swal-cancel",
      denyButton: "cluihud-swal-deny",
    },
    ...opts,
  };
}

export interface ConfirmOptions {
  title: string;
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  kind?: "warning" | "error" | "question";
  destructive?: boolean;
}

/// Yes/no confirmation modal. Returns `true` when the user confirms.
export async function confirm(opts: ConfirmOptions): Promise<boolean> {
  const result: SweetAlertResult = await Swal.fire(
    baseOptions(opts.kind ?? "warning", {
      title: opts.title,
      html: opts.body,
      showCancelButton: true,
      confirmButtonText: opts.confirmLabel ?? "Confirm",
      cancelButtonText: opts.cancelLabel ?? "Cancel",
      reverseButtons: true,
      focusCancel: true,
      customClass: {
        popup: "cluihud-swal-popup",
        title: "cluihud-swal-title",
        htmlContainer: "cluihud-swal-body",
        actions: "cluihud-swal-actions",
        icon: "cluihud-swal-icon",
        confirmButton: opts.destructive
          ? "cluihud-swal-confirm cluihud-swal-confirm-destructive"
          : "cluihud-swal-confirm",
        cancelButton: "cluihud-swal-cancel",
      },
    }),
  );
  return result.isConfirmed;
}

/// Single-button info/error/success notice. Resolves when dismissed.
export async function notify(
  kind: "info" | "success" | "error" | "warning",
  title: string,
  body?: string,
): Promise<void> {
  await Swal.fire(
    baseOptions(kind, {
      title,
      html: body,
      confirmButtonText: "OK",
    }),
  );
}
