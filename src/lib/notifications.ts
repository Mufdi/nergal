import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "./tauri";

export async function notify(title: string, body: string): Promise<void> {
  try {
    const focused = await getCurrentWindow().isFocused();
    if (!focused) {
      await invoke("send_notification", { title, body });
    }
  } catch {
    // silently ignore
  }
}
