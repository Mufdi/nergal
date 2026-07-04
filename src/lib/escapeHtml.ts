/// Required escaper for any value interpolated into a `ConfirmHost` body
/// (rendered via `dangerouslySetInnerHTML` — see `@/lib/confirm`). `&` must
/// go first so the entities emitted by the later replacements aren't
/// themselves re-escaped.
export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
