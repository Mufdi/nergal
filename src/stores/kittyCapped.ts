export const KITTY_CAPPED_TITLE = "Kitty images paused";
export const KITTY_CAPPED_DESCRIPTION = "Reopen this terminal to show them again.";

export interface KittyCappedPayload {
  session_id: string;
  pty_id: string;
}

export function kittyCappedNotice(
  prev: ReadonlySet<string>,
  payload: KittyCappedPayload,
): { notify: boolean; next: ReadonlySet<string> } {
  const key = `${payload.session_id}:${payload.pty_id}`;
  if (prev.has(key)) return { notify: false, next: prev };
  return { notify: true, next: new Set(prev).add(key) };
}
