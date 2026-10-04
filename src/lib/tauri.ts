import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

export function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return tauriListen<T>(event, (e) => handler(e.payload));
}

/// `listen` for effect cleanups: returns a synchronous disposer. Tauri resolves
/// the unlisten fn asynchronously, so a cleanup that runs first would otherwise
/// leave the listener (and its closure) alive forever (BUG-41).
export function subscribe<T>(event: string, handler: (payload: T) => void): () => void {
  let active = true;
  const registration = listen<T>(event, (payload) => {
    if (active) handler(payload);
  });
  return () => {
    active = false;
    void registration.then((fn) => fn()).catch(() => {});
  };
}

let idCounter = 0;
export function generateId(prefix = "pty"): string {
  idCounter += 1;
  return `${prefix}-${Date.now()}-${idCounter}`;
}
