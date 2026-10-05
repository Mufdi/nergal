export type AuxButton = "middle" | "right";

export function auxButtonToForward(button: number, isAltScreen: boolean): AuxButton | null {
  if (!isAltScreen) return null;
  if (button === 1) return "middle";
  if (button === 2) return "right";
  return null;
}
