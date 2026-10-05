export type AuxButton = "middle" | "right";

export interface AuxHeld {
  button: number;
  aux: AuxButton;
  col: number;
  row: number;
}

export function auxButtonToForward(button: number, isAltScreen: boolean): AuxButton | null {
  if (!isAltScreen) return null;
  if (button === 1) return "middle";
  if (button === 2) return "right";
  return null;
}

// One held aux button per terminal: a second press while one is held would
// leave wezterm tracking two buttons whose releases can arrive in any order.
export function auxPress(held: AuxHeld | null, press: AuxHeld): { held: AuxHeld; sendPress: boolean } {
  if (held) return { held, sendPress: false };
  return { held: press, sendPress: true };
}

export function auxRelease(held: AuxHeld | null, button: number): { held: AuxHeld | null; release: AuxHeld | null } {
  if (!held || held.button !== button) return { held, release: null };
  return { held: null, release: held };
}
