import { useState, useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { commandPaletteOpenAtom, shortcutRegistryAtom, type ShortcutAction } from "@/stores/shortcuts";
import { Search } from "lucide-react";

export function CommandPalette() {
  const isOpen = useAtomValue(commandPaletteOpenAtom);
  const setOpen = useSetAtom(commandPaletteOpenAtom);
  const registry = useAtomValue(shortcutRegistryAtom);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => Math.min(prev + 1, filtered.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => Math.max(prev - 1, 0));
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        const action = filtered[selectedIndex];
        if (action) {
          setOpen(false);
          action.handler();
        }
        return;
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, selectedIndex, query]);

  // Scroll selected item into view
  useEffect(() => {
    if (!isOpen || !listRef.current) return;
    const selected = listRef.current.querySelector("[data-palette-selected]") as HTMLElement | null;
    if (selected) selected.scrollIntoView({ block: "nearest" });
  }, [isOpen, selectedIndex]);

  if (!isOpen) return null;

  const lowerQuery = query.toLowerCase();
  const filtered = registry.filter((action) => {
    if (action.id === "command-palette") return false;
    if (!lowerQuery) return true;
    return (
      action.label.toLowerCase().includes(lowerQuery) ||
      action.keywords.some((k) => k.includes(lowerQuery))
    );
  });

  const CONTEXTUAL_SHORTCUTS = [
    { label: "Scroll Down", keys: "↓" },
    { label: "Scroll Up", keys: "↑" },
    { label: "Scroll Page Down", keys: "PageDown" },
    { label: "Scroll Page Up", keys: "PageUp" },
    { label: "Scroll to Top", keys: "Home" },
    { label: "Scroll to Bottom", keys: "End" },
    { label: "Next Spec Sub-Tab", keys: "Shift+→" },
    { label: "Previous Spec Sub-Tab", keys: "Shift+←" },
    { label: "Back from Spec Delta", keys: "Backspace" },
  ];

  const filteredContextual = lowerQuery
    ? CONTEXTUAL_SHORTCUTS.filter((s) => s.label.toLowerCase().includes(lowerQuery) || s.keys.toLowerCase().includes(lowerQuery))
    : CONTEXTUAL_SHORTCUTS;

  const categories = ["navigation", "session", "panel", "action"] as const;
  const grouped = new Map<string, ShortcutAction[]>();
  for (const action of filtered) {
    const list = grouped.get(action.category) ?? [];
    list.push(action);
    grouped.set(action.category, list);
  }

  let flatIndex = 0;

  function handleSelect(action: ShortcutAction) {
    setOpen(false);
    action.handler();
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-center pt-[20vh]" onClick={() => setOpen(false)}>
      <div className="fixed inset-0 bg-scrim cluihud-blur-sm" />
      <div
        className="relative z-10 w-full max-w-lg rounded-lg border-2 border-primary bg-card shadow-lg max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-border/50 px-3 py-2">
          <Search className="size-4 text-muted-foreground shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => { setQuery(e.target.value); setSelectedIndex(0); }}
            placeholder="Type a command..."
            className="flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground/50"
          />
        </div>
        <div ref={listRef} className="flex-1 overflow-y-auto py-1">
          {filtered.length === 0 && (
            <div className="flex items-center justify-center py-6">
              <span className="text-xs text-muted-foreground">No matching commands</span>
            </div>
          )}
          {categories.map((cat) => {
            const actions = grouped.get(cat);
            if (!actions || actions.length === 0) return null;
            return (
              <div key={cat}>
                <div className="px-3 py-1">
                  <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
                    {cat}
                  </span>
                </div>
                {actions.map((action) => {
                  const idx = flatIndex++;
                  const isSelected = idx === selectedIndex;
                  return (
                    <button
                      key={action.id}
                      data-palette-selected={isSelected ? "true" : undefined}
                      onClick={() => handleSelect(action)}
                      onMouseEnter={() => setSelectedIndex(idx)}
                      className={`flex w-full items-center justify-between px-3 py-1.5 text-left transition-colors ${
                        isSelected ? "bg-secondary text-foreground" : "text-foreground/80 hover:bg-secondary/50"
                      }`}
                    >
                      <span className="text-xs">{action.label}</span>
                      <KeyBadges keys={action.keys} />
                    </button>
                  );
                })}
              </div>
            );
          })}
          {filteredContextual.length > 0 && (
            <div>
              <div className="px-3 py-1">
                <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
                  contextual
                </span>
              </div>
              {filteredContextual.map((s) => (
                <div
                  key={s.label}
                  className="flex w-full items-center justify-between px-3 py-1.5 text-muted-foreground"
                >
                  <span className="text-xs">{s.label}</span>
                  <span className="flex items-center gap-0.5">
                    {s.keys.split("+").map((k) => (
                      <kbd key={k} className="rounded bg-secondary px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">
                        {k}
                      </kbd>
                    ))}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function KeyBadges({ keys }: { keys: string }) {
  const parts = keys.split("+").map((p) => {
    if (p === "ctrl") return "Ctrl";
    if (p === "shift") return "Shift";
    if (p === "alt") return "Alt";
    if (p === "tab") return "Tab";
    if (p === "arrowleft") return "\u2190";
    if (p === "arrowright") return "\u2192";
    if (p === "ñ") return "\u00d1";
    return p.toUpperCase();
  });

  return (
    <div className="flex items-center gap-0.5">
      {parts.map((part, i) => (
        <kbd
          key={i}
          className="inline-flex h-5 min-w-5 items-center justify-center rounded bg-background/80 px-1 text-[10px] font-medium text-muted-foreground border border-border/50"
        >
          {part}
        </kbd>
      ))}
    </div>
  );
}
