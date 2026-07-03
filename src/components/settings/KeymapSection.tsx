import { useState, useEffect, useMemo } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import {
  shortcutRegistryAtom,
  resolvedShortcutsAtom,
  keymapCaptureActiveAtom,
  type ShortcutAction,
} from "@/stores/shortcuts";
import { droppedKeymapOverridesAtom, droppedKeymapNoticeDismissedAtom } from "@/stores/keymapMigration";
import {
  LOCKED_SHORTCUT_IDS,
  formatKeyParts,
  eventToKeys,
  validateCombo,
  isChord,
  chordContinuation,
  comboSignature,
  parseKeys,
} from "@/lib/keymap";
import { confirm } from "@/lib/confirm";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { Lock, RotateCcw, AlertTriangle } from "lucide-react";

type Layer = "core" | "surfaces" | "leader" | "app-scope" | "palette-only";

// Ctrl+Space is only contested on macOS (input-source switch) and Windows
// (CJK IME toggles) — on Linux the hint would be noise about other systems.
const LEADER_OS_HINT: string | null = (() => {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent || "";
  if (/Mac|iPad|iPhone|iPod/.test(ua)) {
    return "macOS may reserve Ctrl+Space for input-source switching — rebind here if the leader never fires; Ctrl+. is a good alternative.";
  }
  if (/Windows/.test(ua)) {
    return "CJK IMEs often reserve Ctrl+Space — rebind here if the leader never fires; Ctrl+. is a good alternative.";
  }
  return null;
})();

const LAYER_ORDER: Layer[] = ["core", "surfaces", "leader", "app-scope", "palette-only"];
const LAYER_LABEL: Record<Layer, string> = {
  core: "Core",
  surfaces: "Surfaces",
  leader: "Leader",
  "app-scope": "App-scope",
  "palette-only": "Palette-only",
};

/// Deterministic from the DEFAULT keys + scope (design D9 / spec "Keymap
/// editor supports the layer model"). `keys === ""` is checked first since
/// neither `scope`/`isChord`/`parseKeys` are meaningful on the empty sentinel.
function deriveLayer(action: ShortcutAction): Layer {
  if (action.keys === "") return "palette-only";
  if (action.scope === "app") return "app-scope";
  if (isChord(action.keys)) return "leader";
  const parsed = parseKeys(action.keys);
  if (parsed?.ctrl && parsed?.shift) return "surfaces";
  return "core";
}

const KEYBOARD_OWNERSHIP_OPTIONS = [
  { value: "nergal", label: "Nergal" },
  { value: "agent", label: "Agent CLI" },
];

const KEYBOARD_OWNERSHIP_EXPLANATION: Record<string, string> = {
  nergal: "Nergal claims every shortcut globally; agent shortcuts stay reachable via Ctrl+Space → .",
  agent: "Disputed keys (Ctrl+B/W/S/L, Alt+↑/↓) reach the agent CLI while focus is in the terminal.",
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

/// Renders a leader chord as `Ctrl+Space` (the LIVE effective binding, so an
/// overridden leader shows correctly) followed by the continuation. Falls
/// back to a plain `KeyCombo` for non-chords.
function ChordCombo({ keys, leaderKeys }: { keys: string; leaderKeys: string }) {
  const continuation = chordContinuation(keys);
  if (continuation === null) return <KeyCombo keys={keys} />;
  return (
    <span className="flex items-center gap-1">
      <KeyCombo keys={leaderKeys} />
      <span className="text-[9px] text-muted-foreground/50">then</span>
      <KeyCombo keys={continuation} />
    </span>
  );
}

export function KeymapSection() {
  const [config, setConfig] = useAtom(configAtom);
  const defaults = useAtomValue(shortcutRegistryAtom);
  const effective = useAtomValue(resolvedShortcutsAtom);
  const setCaptureActive = useSetAtom(keymapCaptureActiveAtom);
  const dropped = useAtomValue(droppedKeymapOverridesAtom);
  const [noticeDismissed, setNoticeDismissed] = useAtom(droppedKeymapNoticeDismissedAtom);
  const overrides = config.keymap_overrides ?? {};
  const ownership = config.keyboard_ownership ?? "nergal";

  const [capturingId, setCapturingId] = useState<string | null>(null);
  const [awaitingContinuation, setAwaitingContinuation] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const effectiveList = useMemo(
    () => effective.map((a) => ({ id: a.id, keys: a.keys })),
    [effective],
  );
  const leaderKeys = effectiveList.find((a) => a.id === "leader")?.keys ?? "ctrl+space";

  useEffect(() => {
    if (!capturingId) {
      setCaptureActive(false);
      setAwaitingContinuation(false);
      return;
    }
    setCaptureActive(true);
    setAwaitingContinuation(false);
    // Internal branching uses a ref (not the `awaitingContinuation` state) so
    // the effect — and its single keydown listener — doesn't need to
    // resubscribe between the two capture steps.
    const awaitingRef = { current: false };

    function finalize(keys: string) {
      const v = validateCombo(keys, capturingId!, effectiveList);
      if (!v.ok) {
        setError(v.reason ?? "Invalid combo");
        awaitingRef.current = false;
        setAwaitingContinuation(false);
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
      setAwaitingContinuation(false);
      setError(null);
    }

    function onKey(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturingId(null);
        setAwaitingContinuation(false);
        setError(null);
        return;
      }
      const keys = eventToKeys(e);
      if (!keys) return; // bare modifier or unsupported key — keep listening

      if (!awaitingRef.current) {
        // Step 1: a combo matching the effective leader binding promotes
        // this recording into a two-step chord capture — for ANY row, not
        // just leader-layer ones (recording "leader x" for a currently-
        // global shortcut is legal, same as recording a leader continuation
        // for a leader-layer row). Any other combo finalizes immediately as
        // a plain binding.
        const leaderSig = comboSignature(leaderKeys);
        if (leaderSig && comboSignature(keys) === leaderSig) {
          awaitingRef.current = true;
          setAwaitingContinuation(true);
          setError(null);
          return;
        }
        finalize(keys);
        return;
      }

      // Step 2: the continuation key.
      awaitingRef.current = false;
      finalize(`leader ${keys}`);
    }
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      setCaptureActive(false);
    };
  }, [capturingId, effectiveList, defaults, leaderKeys, setConfig, setCaptureActive]);

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
    const map = new Map<Layer, typeof defaults>();
    for (const layer of LAYER_ORDER) map.set(layer, []);
    for (const action of defaults) {
      map.get(deriveLayer(action))!.push(action);
    }
    return map;
  }, [defaults]);

  const overrideCount = Object.keys(overrides).filter(
    (id) => !LOCKED_SHORTCUT_IDS.has(id),
  ).length;

  const undismissedDrops = dropped.length > 0 && !noticeDismissed;

  return (
    <div className="space-y-4">
      <div className="grid gap-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 px-1">
          Keyboard ownership
        </span>
        <Select
          value={ownership}
          onValueChange={(v) => setConfig((prev) => ({ ...prev, keyboard_ownership: v as "nergal" | "agent" }))}
          options={KEYBOARD_OWNERSHIP_OPTIONS}
        />
        <p className="text-xs text-muted-foreground px-1">
          {KEYBOARD_OWNERSHIP_EXPLANATION[ownership]}
        </p>
      </div>

      {undismissedDrops && (
        <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
          <AlertTriangle size={12} className="mt-0.5 shrink-0" />
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium">
                {dropped.length} keymap override{dropped.length === 1 ? "" : "s"} reset by the shortcuts restructure
              </span>
              <button
                type="button"
                onClick={() => setNoticeDismissed(true)}
                className="shrink-0 text-[10px] underline underline-offset-2 hover:no-underline"
              >
                Dismiss
              </button>
            </div>
            <ul className="space-y-0.5 text-[11px]">
              {dropped.map((d) => (
                <li key={d.id}>
                  <code className="rounded bg-amber-500/20 px-1 font-mono text-[10px]">{d.id}</code>{" "}
                  ({d.keys}) —{" "}
                  {d.reason === "removed-id"
                    ? "this shortcut no longer exists"
                    : "collides with a new default binding"}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-muted-foreground">
          Click <span className="font-medium text-foreground">Rebind</span>, then press the new combo
          (needs Ctrl or Alt, or press <span className="font-medium text-foreground">Ctrl+Space</span> first
          to record a leader continuation). Esc cancels. The command palette reflects your changes.
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

      {LAYER_ORDER.map((layer) => {
        const actions = grouped.get(layer);
        if (!actions || actions.length === 0) return null;
        return (
          <div key={layer} className="space-y-1">
            <div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 px-1">
              {LAYER_LABEL[layer]}
            </div>
            <div className="rounded-md border border-border/40 divide-y divide-border/30">
              {actions.map((action) => {
                const locked = LOCKED_SHORTCUT_IDS.has(action.id);
                const overridden = !locked && !!overrides[action.id];
                const eff = effectiveList.find((e) => e.id === action.id);
                const keys = eff?.keys ?? action.keys;
                const isCapturing = capturingId === action.id;
                return (
                  <div key={action.id}>
                    <div className="flex items-center justify-between gap-3 px-3 py-1.5">
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
                          <span className="text-[11px] text-primary animate-pulse">
                            {awaitingContinuation ? "Ctrl+Space + …" : "Press keys…"}
                          </span>
                        ) : (
                          <ChordCombo keys={keys} leaderKeys={leaderKeys} />
                        )}
                        {locked ? (
                          <Tooltip>
                            <TooltipTrigger render={<span className="inline-flex items-center text-muted-foreground/50" />}>
                              <Lock size={12} />
                            </TooltipTrigger>
                            <TooltipContent>This shortcut is structural and can't be remapped</TooltipContent>
                          </Tooltip>
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
                            <Tooltip>
                              <TooltipTrigger
                                render={
                                  <button
                                    type="button"
                                    onClick={() => resetOne(action.id)}
                                    disabled={!overridden}
                                    className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors disabled:opacity-30 disabled:hover:bg-transparent outline-none focus:ring-1 focus:ring-inset focus:ring-primary/70"
                                  />
                                }
                              >
                                <RotateCcw size={12} />
                              </TooltipTrigger>
                              <TooltipContent>Reset to default</TooltipContent>
                            </Tooltip>
                          </>
                        )}
                      </span>
                    </div>
                    {action.id === "leader" && LEADER_OS_HINT && (
                      <p className="px-3 pb-1.5 text-[10px] text-muted-foreground/50">
                        {LEADER_OS_HINT}
                      </p>
                    )}
                    {action.id === "focus-terminal" && (
                      <p className="px-3 pb-1.5 text-[10px] text-muted-foreground/50">
                        Bound to the physical key right of L (Ñ on Spanish layouts, ; elsewhere) — rebind it
                        here if that corner is awkward on your keyboard.
                      </p>
                    )}
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
