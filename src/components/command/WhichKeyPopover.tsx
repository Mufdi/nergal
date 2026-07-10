import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { leaderPendingAtom } from "@/stores/leader";
import { resolvedShortcutsAtom, type ShortcutAction } from "@/stores/shortcuts";
import { scratchpadOpacityAtom } from "@/stores/scratchpad";
import { isChord, chordContinuation } from "@/lib/keymap";
import { Kbd } from "@/components/ui/kbd";

/// which-key.nvim convention (design D1): fluent users resolve the chord
/// before this elapses, so the popover's visuals must never mount for them.
const WHICH_KEY_DELAY_MS = 150;

/// Fixed column order for the grouped listing (spec: "grouped by family").
/// Groups absent from the current registry are skipped; any group value not
/// listed here still renders, appended after the known ones, so a future
/// group addition degrades gracefully instead of vanishing.
const GROUP_ORDER = ["surfaces", "git", "vault", "session"];
const GROUP_LABELS: Record<string, string> = {
  surfaces: "Surfaces",
  git: "Git",
  vault: "Vault",
  session: "Session",
};

export function WhichKeyPopover() {
  const pending = useAtomValue(leaderPendingAtom);
  const registry = useAtomValue(resolvedShortcutsAtom);
  const opacity = useAtomValue(scratchpadOpacityAtom);
  const [visible, setVisible] = useState(false);

  // Same translucency treatment as the scratchpad (user request): color-mix
  // the theme's --card token so the popover shows the terminal faintly behind.
  const cardBg = `color-mix(in srgb, var(--card) ${opacity * 100}%, transparent)`;

  useEffect(() => {
    if (!pending) {
      setVisible(false);
      return;
    }
    const timer = setTimeout(() => setVisible(true), WHICH_KEY_DELAY_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  if (!pending || !visible) return null;

  const leaderKeys = registry.find((a) => a.id === "leader")?.keys ?? "ctrl+space";

  if (pending.mode === "raw") {
    return (
      <div className="fixed inset-x-0 bottom-8 z-[60] flex justify-center">
        <div
          className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-[11px] text-foreground shadow-2xl"
          style={{ background: cardBg }}
        >
          <Kbd keys={leaderKeys} />
          <Kbd keys="." />
          <span className="text-muted-foreground">next keystroke goes to the terminal</span>
        </div>
      </div>
    );
  }

  // Derived exclusively from the resolved registry — never a hardcoded list
  // (spec requirement), so remaps and future leader continuations show up
  // for free.
  const chordEntries = registry.filter((a) => isChord(a.keys));
  const groups = new Map<string, ShortcutAction[]>();
  for (const entry of chordEntries) {
    const key = entry.group ?? "other";
    const list = groups.get(key) ?? [];
    list.push(entry);
    groups.set(key, list);
  }
  const orderedGroups = [
    ...GROUP_ORDER.filter((g) => groups.has(g)),
    ...[...groups.keys()].filter((g) => !GROUP_ORDER.includes(g)),
  ];

  return (
    <div className="fixed inset-x-0 bottom-8 z-[60] flex justify-center">
      <div
        className="max-w-[90vw] rounded-lg border border-border p-2 shadow-2xl"
        style={{ background: cardBg }}
      >
        <div className="mb-1.5 flex flex-wrap items-center gap-1.5 px-1">
          <Kbd keys={leaderKeys} />
          <span className="text-[10px] text-muted-foreground">…</span>
          {pending.hintNonContinuation && (
            <span className="text-[10px] text-muted-foreground">
              Not a leader key — press <Kbd keys="." /> to send the next keystroke to the terminal
            </span>
          )}
        </div>
        <div className="grid auto-cols-max grid-flow-col gap-x-4 gap-y-0.5">
          {orderedGroups.map((group) => (
            <div key={group} className="flex flex-col gap-0.5">
              <span className="px-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                {GROUP_LABELS[group] ?? group}
              </span>
              {groups.get(group)!.map((entry) => (
                <div key={entry.id} className="flex items-center gap-1.5 px-1 py-0.5">
                  <Kbd keys={chordContinuation(entry.keys) ?? ""} />
                  <span className="truncate text-[11px] text-foreground/80">{entry.label}</span>
                </div>
              ))}
            </div>
          ))}
          {/* Raw mode's `.` is dispatcher semantics, not a registry entry —
              this one static row is expected (D2). */}
          <div className="flex flex-col gap-0.5">
            <span className="px-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Terminal
            </span>
            <div className="flex items-center gap-1.5 px-1 py-0.5">
              <Kbd keys="." />
              <span className="truncate text-[11px] text-foreground/80">Send to terminal</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
