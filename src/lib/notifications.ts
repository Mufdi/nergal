import { sendNotification, isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { getCurrentWindow } from "@tauri-apps/api/window";

let permissionChecked = false;
let hasPermission = false;

async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return hasPermission;
  permissionChecked = true;
  hasPermission = await isPermissionGranted();
  if (!hasPermission) {
    const result = await requestPermission();
    hasPermission = result === "granted";
  }
  return hasPermission;
}

export async function notify(title: string, body: string): Promise<void> {
  try {
    const focused = await getCurrentWindow().isFocused();
    if (focused) return;

    const granted = await ensurePermission();
    if (!granted) return;

    sendNotification({ title, body });
  } catch {
    // silently ignore notification errors
  }
}
