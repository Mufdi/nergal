import { appStore } from "@/stores/jotaiStore";
import { focusZoneAtom } from "@/stores/shortcuts";

/// Mount-time focus for right-panel content. These components also mount
/// when a session switch restores a panel view or tab — focusing there
/// steals the prompt from the terminal (BUG-09 v0.2.0). Intentional opens
/// (TopBar click, panel shortcuts) set the zone to "panel" first, so
/// gating on the zone separates the two paths.
export function focusIfPanelZone(el: HTMLElement | null | undefined): void {
  if (appStore.get(focusZoneAtom) === "panel") el?.focus();
}
