/// Compact inline state pill + dropdown for Linear panel rows. Follows the
/// same keyboard model as StatePickerRail in LinearTaskView: while open, a
/// window-capture listener owns ↑/↓/Enter/Esc and stopImmediatePropagation
/// beats the row's keyboard handlers and the panel Esc-to-close.
///
/// Uses e.key (not e.code) — WebKitGTK native events don't populate `code`
/// for dropdown keyboard events.

import { useEffect, useRef, useState } from "react";
import { LinearStatusIcon } from "@/components/linear/LinearStatusIcon";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { type WorkflowStateView } from "@/stores/linear";

export function StatePicker({
  currentStateId,
  currentStateName,
  currentStateType,
  currentStateColor,
  states,
  loading,
  onSelect,
  pending,
  open,
  onOpenChange,
}: {
  currentStateId?: string;
  currentStateName?: string;
  currentStateType?: string;
  currentStateColor?: string;
  states: WorkflowStateView[];
  loading: boolean;
  onSelect: (stateId: string) => void;
  pending: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  function choose(stateId: string) {
    onOpenChange(false);
    onSelect(stateId);
  }

  useEffect(() => {
    if (!open) return;
    const idx = states.findIndex((s) => s.id === currentStateId);
    setActiveIdx(idx >= 0 ? idx : 0);
  }, [open, currentStateId, states]);

  // Scroll highlighted option into view when navigating by keyboard.
  useEffect(() => {
    if (!open) return;
    wrapRef.current
      ?.querySelector<HTMLElement>(`[data-opt-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [open, activeIdx]);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
    }
    function onKey(e: KeyboardEvent) {
      if (states.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % states.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + states.length) % states.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        const s = states[activeIdx];
        if (s) choose(s.id);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        onOpenChange(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, states, activeIdx]);

  const color = currentStateColor ?? null;

  return (
    <div ref={wrapRef} className="relative">
      {/* span[role=button], not <button>: this picker renders inside panel rows
          that are themselves <button> — nested buttons are invalid HTML (same
          convention as the row chevron / copy-id affordances). Provider comes
          from LinearPanel (the only mount site). */}
      <Tooltip>
        <TooltipTrigger
          render={
            <span
              role="button"
              tabIndex={-1}
              onClick={() => onOpenChange(!open)}
              className={`flex cursor-pointer items-center gap-1 rounded px-1 py-0.5 outline-none transition-colors hover:bg-secondary/40 ${pending ? "opacity-60" : ""}`}
            />
          }
        >
          <LinearStatusIcon
            stateType={currentStateType}
            color={color}
            size={13}
            className="shrink-0"
          />
        </TooltipTrigger>
        <TooltipContent>{currentStateName ?? "No state"}</TooltipContent>
      </Tooltip>
      {open && (
        <div
          data-floating-popup
          className="absolute left-0 top-full z-50 mt-1 min-w-[160px] rounded-md border border-border bg-card shadow-md"
        >
          <div className="max-h-48 overflow-y-auto py-0.5">
            {loading ? (
              <div className="px-2 py-1.5 text-[10px] text-muted-foreground">Loading states…</div>
            ) : states.length === 0 ? (
              <div className="px-2 py-1 text-[10px] text-muted-foreground">No states available.</div>
            ) : (
              states.map((s, i) => (
                <span
                  key={s.id}
                  role="button"
                  tabIndex={-1}
                  data-opt-idx={i}
                  data-nav-selected={i === activeIdx || undefined}
                  onMouseEnter={() => setActiveIdx(i)}
                  onClick={() => choose(s.id)}
                  className={`flex w-full cursor-pointer items-center gap-2 px-2 py-1 text-left text-[11px] transition-colors data-[nav-selected=true]:bg-accent data-[nav-selected=true]:text-accent-foreground ${
                    s.id === currentStateId ? "text-foreground" : "text-muted-foreground"
                  }`}
                >
                  <LinearStatusIcon stateType={s.type} color={s.color ?? null} size={12} className="shrink-0" />
                  {s.name}
                </span>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
