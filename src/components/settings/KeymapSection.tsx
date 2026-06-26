import { useState, useEffect, useMemo } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import {
  shortcutRegistryAtom,
  resolvedShortcutsAtom,
  keymapCaptureActiveAtom,
} from "@/stores/shortcuts";
import { LOCKED_SHORTCUT_IDS, formatKeyParts, eventToKeys, validateCombo } from "@/lib/keymap";
import { confirm } from "@/lib/confirm";
import { Button } from "@/components/ui/button";
import { Lock, RotateCcw } from "lucide-react";

const CATEGORY_ORDER = ["navigation", "session", "panel", "action"] as const;
const CATEGORY_LABEL: Record<string, string> = {
  navigation: "Navigation",
  session: "Session",
  panel: "Panel",
  action: "Action",
};

function KeyCombo({ keys }: { keys: string }) {
  return (
    <span className="flex items-center gap-0.5">
      {formatKeyParts(keys).map((part, i) => (
        <kbd
          key={i}
          className="inline-flex h-5 min-w-5 items-center justify-center rounded bg-background/80 px-1 text-[10px] font-medium text-muted-foreground border border-border/50"
        >
          {part}
        </kbd>
      ))}
    </span>
  );
}

export function KeymapSection() {
  const [config, setConfig] = useAtom(configAtom);
  const defaults = useAtomValue(shortcutRegistryAtom);
  const effective = useAtomValue(resolvedShortcutsAtom);
  const setCaptureActive = useSetAtom(keymapCaptureActiveAtom);
  const overrides = config.keymap_overrides ?? {};

  const [capturingId, setCapturingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const effectiveList = useMemo(
    () => effective.map((a) => ({ id: a.id, keys: a.keys })),
    [effective],
  );

  useEffect(() => {
    if (!capturingId) {
      setCaptureActive(false);
      return;
    }
    setCaptureActive(true);
    function onKey(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturingId(null);
        setError(null);
        return;
      }
      const keys = eventToKeys(e);
      if (!keys) return; // bare modifier or unsupported key — keep listening
      const v = validateCombo(keys, capturingId!, effectiveList);
      if (!v.ok) {
        setError(v.reason ?? "Invalid combo");
        return;
      }
      const def = defaults.find((d) => d.id === capturingId);
      setConfig((prev) => {
        const next = { ...(prev.keymap_overrides ?? {}) };
        // Match-default clears the override so config.json stays minimal.
        if (def && def.keys === keys) delete next[capturingId!];
        else next[capturingId!] = keys;
        return { ...prev, keymap_overrides: next };
      });
      setCapturingId(null);
      setError(null);
    }
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      setCaptureActive(false);
    };
  }, [capturingId, effectiveList, defaults, setConfig, setCaptureActive]);

  function resetOne(id: string) {
    setConfig((prev) => {
      const next = { ...(prev.keymap_overrides ?? {}) };
      delete next[id];
      return { ...prev, keymap_overrides: next };
    });
  }

  async function resetAll() {
    const ok = await confirm({
      title: "Reset all shortcuts?",
      body: "Every custom keybinding returns to its default. This can't be undone.",
      confirmLabel: "Reset all",
      destructive: true,
    });
    if (!ok) return;
    setConfig((prev) => ({ ...prev, keymap_overrides: {} }));
  }

  const grouped = useMemo(() => {
    const map = new Map<string, typeof defaults>();
    for (const cat of CATEGORY_ORDER) map.set(cat, []);
    for (const action of defaults) {
      const list = map.get(action.category);
      if (list) list.push(action);
    }
    return map;
  }, [defaults]);

  const overrideCount = Object.keys(overrides).filter(
    (id) => !LOCKED_SHORTCUT_IDS.has(id),
  ).length;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-muted-foreground">
          Click <span className="font-medium text-foreground">Rebind</span>, then press the new combo
          (needs Ctrl or Alt). Esc cancels. The command palette reflects your changes.
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={resetAll}
          disabled={overrideCount === 0}
          className="shrink-0"
        >
          <RotateCcw size={13} className="mr-1.5" />
          Reset all
        </Button>
      </div>

      {CATEGORY_ORDER.map((cat) => {
        const actions = grouped.get(cat);
        if (!actions || actions.length === 0) return null;
        return (
          <div key={cat} className="space-y-1">
            <div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 px-1">
              {CATEGORY_LABEL[cat]}
            </div>
            <div className="rounded-md border border-border/40 divide-y divide-border/30">
              {actions.map((action) => {
                const locked = LOCKED_SHORTCUT_IDS.has(action.id);
                const overridden = !locked && !!overrides[action.id];
                const eff = effectiveList.find((e) => e.id === action.id);
                const keys = eff?.keys ?? action.keys;
                const isCapturing = capturingId === action.id;
                return (
                  <div
                    key={action.id}
                    className="flex items-center justify-between gap-3 px-3 py-1.5"
                  >
                    <span className="flex min-w-0 items-center gap-1.5 text-xs text-foreground/90">
                      <span className="truncate">{action.label}</span>
                      {overridden && (
                        <span className="shrink-0 text-[9px] uppercase tracking-wide text-primary/80">
                          custom
                        </span>
                      )}
                    </span>
                    <span className="flex items-center gap-2 shrink-0">
                      {isCapturing ? (
                        <span className="text-[11px] text-primary animate-pulse">Press keys…</span>
                      ) : (
                        <KeyCombo keys={keys} />
                      )}
                      {locked ? (
                        <span
                          className="inline-flex items-center text-muted-foreground/50"
                          title="This shortcut is structural and can't be remapped"
                        >
                          <Lock size={12} />
                        </span>
                      ) : (
                        <>
                          <button
                            type="button"
                            onClick={() => {
                              setError(null);
                              setCapturingId(isCapturing ? null : action.id);
                            }}
                            className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors outline-none focus:ring-1 focus:ring-inset focus:ring-primary/70"
                          >
                            {isCapturing ? "Cancel" : "Rebind"}
                          </button>
                          <button
                            type="button"
                            onClick={() => resetOne(action.id)}
                            disabled={!overridden}
                            title="Reset to default"
                            className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors disabled:opacity-30 disabled:hover:bg-transparent outline-none focus:ring-1 focus:ring-inset focus:ring-primary/70"
                          >
                            <RotateCcw size={12} />
                          </button>
                        </>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}

      {error && (
        <p className="text-xs text-destructive px-1" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
