/// Inline status pill that expands to a mini-picker. Controlled open state so
/// the detail's Enter can open it; the open dropdown owns ↑/↓ (cycle options,
/// wraparound) + Enter (select) + Esc (close). Selecting refocuses the trigger
/// so arrow-nav resumes.

import { useEffect, useRef, useState } from "react";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { PulseDots } from "@/components/ui/PulseDots";
import { statusFraction, type ClickUpListStatus } from "@/stores/clickup";

export function StatusPicker({
  currentStatus,
  currentColor,
  currentType,
  statuses,
  loading,
  onSelect,
  pending,
  open,
  onOpenChange,
  navSelected,
}: {
  currentStatus: string;
  currentColor: string | null;
  currentType: string | null;
  statuses: ClickUpListStatus[];
  loading: boolean;
  onSelect: (name: string) => void;
  pending: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  navSelected: boolean;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  function choose(name: string) {
    onOpenChange(false);
    onSelect(name);
  }

  useEffect(() => {
    if (!open) return;
    const cur = statuses.findIndex((s) => s.name === currentStatus);
    setActiveIdx(cur >= 0 ? cur : 0);
  }, [open, statuses, currentStatus]);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
    }
    function onKey(e: KeyboardEvent) {
      if (statuses.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % statuses.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + statuses.length) % statuses.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        const s = statuses[activeIdx];
        if (s) choose(s.name);
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
  }, [open, statuses, activeIdx]);

  return (
    <div ref={wrapRef} className="relative">
      {/* span[role=button], not <button>: this picker renders inside panel rows
          that are themselves <button> — nested buttons are invalid HTML (same
          convention as the row chevron / copy-id affordances). */}
      <span
        role="button"
        tabIndex={-1}
        data-nav-key="status"
        onClick={() => onOpenChange(!open)}
        className={`flex cursor-pointer items-center gap-1 rounded-full px-2 py-0.5 text-[11px] leading-tight outline-none transition-colors hover:opacity-80 ${navSelected ? "ring-1 ring-foreground/50" : ""} ${pending ? "opacity-60" : ""}`}
        style={{
          background: currentColor ? `${currentColor}26` : "var(--color-secondary)",
          color: currentColor ?? "var(--color-secondary-foreground)",
        }}
      >
        <StatusIcon
          type={currentType}
          color={currentColor}
          fraction={statusFraction(statuses, currentStatus)}
          size={11}
          className="shrink-0"
        />
        {currentStatus}
        {pending && <PulseDots count={1} dotClassName="size-1" />}
      </span>
      {open && (
        <div
          data-floating-popup
          className="absolute left-0 top-full z-50 mt-1 min-w-[140px] rounded border border-border bg-card py-1 shadow-lg"
        >
          {loading ? (
            <div className="flex flex-col gap-1 px-2.5 py-1.5">
              <span className="text-[10px] text-muted-foreground">Loading statuses…</span>
              <ProgressBar />
            </div>
          ) : statuses.length === 0 ? (
            <div className="px-2.5 py-1 text-[10px] text-muted-foreground">No statuses available.</div>
          ) : (
            statuses.map((s, i) => (
              <span
                key={s.name}
                role="button"
                tabIndex={-1}
                data-nav-selected={i === activeIdx || undefined}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => choose(s.name)}
                className={`flex w-full cursor-pointer items-center gap-1.5 px-2.5 py-1 text-left text-[10px] transition-colors data-[nav-selected=true]:bg-secondary/60 ${
                  s.name === currentStatus ? "text-foreground" : "text-muted-foreground"
                }`}
              >
                <StatusIcon
                  type={s.status_type}
                  color={s.color}
                  fraction={statuses.length > 1 ? i / (statuses.length - 1) : 0.5}
                  size={11}
                  className="shrink-0"
                />
                {s.name}
              </span>
            ))
          )}
        </div>
      )}
    </div>
  );
}
