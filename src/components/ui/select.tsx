"use client";

import { Select as SelectPrimitive } from "@base-ui/react/select";
import { Check, ChevronDown } from "lucide-react";

import { cn } from "@/lib/utils";

export interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  className?: string;
  id?: string;
  disabled?: boolean;
}

export function Select({
  value,
  onValueChange,
  options,
  placeholder = "Select…",
  className,
  id,
  disabled,
}: SelectProps) {
  return (
    <SelectPrimitive.Root
      value={value}
      onValueChange={(v) => onValueChange((v as string) ?? "")}
      disabled={disabled}
    >
      <SelectPrimitive.Trigger
        id={id}
        // Capture phase so this handler runs BEFORE Base UI Select's own
        // bubble-phase keydown listener (which would otherwise open the
        // popup on ArrowDown/Up). preventDefault + stopPropagation here
        // means Base UI never sees the arrow key.
        onKeyDownCapture={(e) => {
          if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
          e.preventDefault();
          e.stopPropagation();
          const root =
            (e.currentTarget as HTMLElement).closest<HTMLElement>(
              '[data-theme-editor], [data-slot="dialog-content"], [role="dialog"]',
            ) ?? document.body;
          // Exclude hidden inputs (Base UI Select renders a form-aware
          // `type="hidden"` next to the trigger) and tabindex=-1 elements
          // so ArrowDown/Up don't land on invisible focus targets.
          const focusables = Array.from(
            root.querySelectorAll<HTMLElement>(
              'input:not([type="hidden"]):not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), [role="combobox"]:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), button:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"])',
            ),
          );
          const idx = focusables.indexOf(e.currentTarget as HTMLElement);
          if (idx === -1) return;
          const nextIdx =
            e.key === "ArrowDown"
              ? Math.min(idx + 1, focusables.length - 1)
              : Math.max(idx - 1, 0);
          if (nextIdx === idx) return;
          focusables[nextIdx].focus({ preventScroll: true });
          focusables[nextIdx].scrollIntoView({ block: "nearest" });
        }}
        className={cn(
          "cluihud-focus-ring flex h-9 w-full items-center justify-between rounded-md border border-input bg-secondary text-foreground px-3 py-1 text-sm shadow-xs transition-colors",
          "disabled:cursor-not-allowed disabled:opacity-60",
          className,
        )}
      >
        <SelectPrimitive.Value placeholder={placeholder}>
          {(val) => {
            const found = options.find((o) => o.value === (val as string));
            return found?.label ?? placeholder;
          }}
        </SelectPrimitive.Value>
        <SelectPrimitive.Icon className="ml-2 text-muted-foreground">
          <ChevronDown size={14} />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>

      <SelectPrimitive.Portal>
        <SelectPrimitive.Positioner sideOffset={4} className="z-[60] outline-none">
          <SelectPrimitive.Popup
            className={cn(
              "min-w-[var(--anchor-width)] overflow-hidden rounded-md border border-border bg-popover text-popover-foreground shadow-lg outline-none",
              "data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95",
              "data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
            )}
          >
            <SelectPrimitive.List className="max-h-[18rem] overflow-y-auto p-1">
              {options.map((opt) => (
                <SelectPrimitive.Item
                  key={opt.value}
                  value={opt.value}
                  className={cn(
                    "relative flex cursor-pointer select-none items-center rounded-sm px-2 py-1.5 pr-7 text-sm outline-none",
                    "data-highlighted:bg-accent data-highlighted:text-accent-foreground",
                    "data-selected:font-medium",
                  )}
                >
                  <SelectPrimitive.ItemText>{opt.label}</SelectPrimitive.ItemText>
                  <SelectPrimitive.ItemIndicator className="absolute right-2 inline-flex">
                    <Check size={14} />
                  </SelectPrimitive.ItemIndicator>
                </SelectPrimitive.Item>
              ))}
            </SelectPrimitive.List>
          </SelectPrimitive.Popup>
        </SelectPrimitive.Positioner>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
