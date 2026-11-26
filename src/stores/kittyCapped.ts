export const KITTY_CAPPED_MESSAGE =
  "Imágenes kitty pausadas en esta terminal. Ábrela de nuevo para volver a verlas.";

export interface KittyCappedPayload {
  session_id: string;
  pty_id: string;
}

export function kittyCappedNotice(
  prev: ReadonlySet<string>,
  payload: KittyCappedPayload,
): { notify: boolean; next: Set<string> } {
  const key = `${payload.session_id}:${payload.pty_id}`;
  if (prev.has(key)) return { notify: false, next: new Set(prev) };
  return { notify: true, next: new Set(prev).add(key) };
}
