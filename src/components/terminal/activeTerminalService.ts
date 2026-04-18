/// Façade over the legacy xterm.js and the wezterm canvas terminal services.
/// Call sites that don't care which renderer is active (session switching,
/// focus plumbing, resume-vs-continue logic) import from here so adding a
/// new terminal path later stays a local change.
///
/// Both services maintain their own `sessionId -> terminal` maps but only
/// one is populated at any given time (gated by `experimental_wezterm_terminal`).
/// Union semantics are safe: `hasTerminal` is true if either service knows
/// the session, and `focusActive` is a no-op on the service that has no
/// active entry.

import * as legacy from "./terminalService";
import * as wez from "./wezterm/wezTerminalService";

export function hasTerminal(sessionId: string): boolean {
  return legacy.hasTerminal(sessionId) || wez.hasTerminal(sessionId);
}

export function focusActive(): void {
  legacy.focusActive();
  wez.focusActive();
}

export async function writeToSession(sessionId: string, text: string): Promise<void> {
  if (wez.hasTerminal(sessionId)) {
    await wez.writeToSession(sessionId, text);
    return;
  }
  await legacy.writeToSession(sessionId, text);
}

export function destroy(sessionId: string): void {
  if (wez.hasTerminal(sessionId)) {
    wez.destroy(sessionId);
    return;
  }
  legacy.destroy(sessionId);
}
