import { atom, type createStore } from "jotai";
import { invoke } from "@/lib/tauri";
import { configAtom, configLoadedAtom } from "./config";
import { toastsAtom } from "./toast";
import type { FocusZone } from "./shortcuts";

type Store = ReturnType<typeof createStore>;

function readingFlagAtom(key: "reading_mode_terminal" | "reading_mode_markdown") {
  return atom(
    (get) => get(configAtom)[key] ?? false,
    (get, set, update: boolean | ((prev: boolean) => boolean)) => {
      const prev = get(configAtom)[key] ?? false;
      const next = typeof update === "function" ? update(prev) : update;
      set(configAtom, (cfg) => ({ ...cfg, [key]: next }));
      invoke("save_config", { config: { [key]: next } }).catch((err) =>
        console.error(`[reading-mode] persisting ${key} failed:`, err),
      );
    },
  );
}

export const readingTerminalAtom = readingFlagAtom("reading_mode_terminal");
export const readingMarkdownAtom = readingFlagAtom("reading_mode_markdown");
export const readingFullWidthTablesAtom = atom((get) => get(configAtom).reading_mode_full_width_tables ?? true);

export function readingModeTarget(zone: FocusZone): "terminal" | "markdown" | null {
  if (zone === "terminal") return "terminal";
  if (zone === "panel") return "markdown";
  return null;
}

export function toggleReadingMode(store: Store, zone: FocusZone): void {
  const target = readingModeTarget(zone);
  if (!target || !store.get(configLoadedAtom)) return;
  const flag = target === "terminal" ? readingTerminalAtom : readingMarkdownAtom;
  const next = !store.get(flag);
  store.set(flag, next);
  store.set(toastsAtom, {
    message: `Reading mode: ${target} ${next ? "on" : "off"}`,
    type: "info",
  });
}
