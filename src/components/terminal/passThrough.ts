import { isYieldedToAgent } from "@/lib/agentYield";

export function shouldPassThrough(
  e: { type: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; code: string },
): boolean {
  // Same shortcut set the legacy terminalService.wireIMEFix filters — these
  // are handled by nergal globally, not by the terminal.
  if (e.type !== "keydown") return true;
  // The dispatcher already declined this key so the agent gets it
  // (keyboard_ownership "agent"); the table below would drop it.
  if (isYieldedToAgent(e)) return false;
  if (e.ctrlKey && !e.shiftKey && !e.altKey) {
    if (
      e.code === "Semicolon" ||
      e.code === "KeyK" ||
      e.code === "KeyB" ||
      e.code === "KeyS" ||
      e.code === "KeyW" ||
      e.code === "KeyN" ||
      e.code === "Tab"
    ) {
      return true;
    }
    if (e.code >= "Digit1" && e.code <= "Digit9") return true;
  }
  if (e.ctrlKey && e.shiftKey && !e.altKey) {
    if (
      [
        "KeyB","KeyP","KeyF","KeyD","KeyS","KeyG","KeyK","KeyL","KeyT",
        "KeyE","KeyM","KeyN","KeyC","KeyI","KeyJ","KeyO","KeyX","KeyA",
        "KeyR","KeyU",
      ].includes(e.code)
    ) {
      return true;
    }
  }
  if (e.altKey && e.code.startsWith("Arrow")) return true;
  return false;
}
