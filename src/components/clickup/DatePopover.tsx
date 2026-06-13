import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

/// Local-timezone calendar popover for the task due date. Replaces the native
/// `<input type=date>` whose WebKitGTK popup never closes on selection and
/// renders the OS-generic chrome. All math is local-TZ: a calendar date is
/// stored as that date's local noon (±12h margin keeps it on the right day
/// regardless of the workspace timezone), and ClickUp is told `due_date_time`
/// is false so it renders date-only.

const WEEKDAYS = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/// Local noon for a calendar date — the value persisted as the due date.
export function localNoonMs(year: number, month: number, day: number): number {
  return new Date(year, month, day, 12, 0, 0, 0).getTime();
}

function sameLocalDay(ms: number, year: number, month: number, day: number): boolean {
  const d = new Date(ms);
  return d.getFullYear() === year && d.getMonth() === month && d.getDate() === day;
}

export function DatePopover({
  valueMs,
  onSelect,
  open,
  onOpenChange,
  navSelected = false,
  disabled = false,
}: {
  valueMs: number | null;
  onSelect: (ms: number | null) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  navSelected?: boolean;
  disabled?: boolean;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);

  /// Commit a value and close. Focus stays on the detail's nav container (the
  /// index cursor), so arrow-nav resumes from the due-date row.
  function commit(ms: number | null) {
    onSelect(ms);
    onOpenChange(false);
  }
  // The month currently shown in the grid; seeded from the value (or today).
  const seed = valueMs != null ? new Date(valueMs) : new Date();
  const [view, setView] = useState({ year: seed.getFullYear(), month: seed.getMonth() });

  // Re-seed the visible month whenever the popover opens.
  useEffect(() => {
    if (!open) return;
    const base = valueMs != null ? new Date(valueMs) : new Date();
    setView({ year: base.getFullYear(), month: base.getMonth() });
  }, [open, valueMs]);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        onOpenChange(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open, onOpenChange]);

  const cells = useMemo(() => {
    const first = new Date(view.year, view.month, 1);
    const leading = first.getDay();
    const daysInMonth = new Date(view.year, view.month + 1, 0).getDate();
    const out: (number | null)[] = [];
    for (let i = 0; i < leading; i++) out.push(null);
    for (let d = 1; d <= daysInMonth; d++) out.push(d);
    return out;
  }, [view]);

  const today = new Date();

  function pick(day: number) {
    commit(localNoonMs(view.year, view.month, day));
  }

  const label =
    valueMs != null
      ? `due ${new Date(valueMs).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`
      : "set due date";

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        data-nav-key="due"
        disabled={disabled}
        onClick={() => onOpenChange(!open)}
        className={`rounded px-1 leading-4 outline-none transition-colors hover:text-foreground disabled:opacity-50 ${navSelected ? "bg-secondary/70 text-foreground" : ""}`}
      >
        {label}
      </button>
      {open && (
        <div data-floating-popup className="absolute left-0 top-full z-50 mt-1 w-56 rounded-md border border-border bg-card p-2 shadow-lg">
          <div className="mb-1 flex items-center justify-between">
            <button
              type="button"
              aria-label="Previous month"
              onClick={() =>
                setView((v) => (v.month === 0 ? { year: v.year - 1, month: 11 } : { year: v.year, month: v.month - 1 }))
              }
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            >
              <ChevronLeft size={13} />
            </button>
            <span className="text-[11px] font-medium text-foreground">
              {MONTHS[view.month]} {view.year}
            </span>
            <button
              type="button"
              aria-label="Next month"
              onClick={() =>
                setView((v) => (v.month === 11 ? { year: v.year + 1, month: 0 } : { year: v.year, month: v.month + 1 }))
              }
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            >
              <ChevronRight size={13} />
            </button>
          </div>
          <div className="grid grid-cols-7 gap-0.5">
            {WEEKDAYS.map((w) => (
              <span key={w} className="flex h-5 items-center justify-center text-[9px] font-medium text-muted-foreground">
                {w}
              </span>
            ))}
            {cells.map((day, i) => {
              if (day === null) return <span key={`pad-${i}`} />;
              const isSelected = valueMs != null && sameLocalDay(valueMs, view.year, view.month, day);
              const isToday = sameLocalDay(today.getTime(), view.year, view.month, day);
              return (
                <button
                  key={day}
                  type="button"
                  onClick={() => pick(day)}
                  className={`flex h-6 items-center justify-center rounded text-[11px] transition-colors ${
                    isSelected
                      ? "bg-primary text-primary-foreground"
                      : isToday
                        ? "text-primary hover:bg-secondary/60"
                        : "text-foreground/80 hover:bg-secondary/60"
                  }`}
                >
                  {day}
                </button>
              );
            })}
          </div>
          <div className="mt-1.5 flex items-center justify-between border-t border-border/40 pt-1.5 text-[10px]">
            <button
              type="button"
              onClick={() => {
                // Pick today regardless of the month currently in view.
                commit(localNoonMs(today.getFullYear(), today.getMonth(), today.getDate()));
              }}
              className="rounded px-1 py-0.5 text-muted-foreground hover:text-foreground"
            >
              Today
            </button>
            {valueMs != null && (
              <button
                type="button"
                onClick={() => commit(null)}
                className="rounded px-1 py-0.5 text-muted-foreground hover:text-red-400"
              >
                Clear
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
