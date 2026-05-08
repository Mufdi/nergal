import { forwardRef } from "react";
import { cn } from "@/lib/utils";

interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  id?: string;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}

export const Switch = forwardRef<HTMLButtonElement, SwitchProps>(function Switch(
  { checked, onCheckedChange, id, disabled, className, "aria-label": ariaLabel },
  ref,
) {
  return (
    <button
      ref={ref}
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "cluihud-focus-ring relative inline-flex h-4 w-7 shrink-0 cursor-pointer items-center rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        checked
          ? "bg-primary border-primary"
          : "bg-secondary border-border",
        className,
      )}
    >
      <span
        aria-hidden
        className={cn(
          "inline-block size-3 rounded-full transition-transform",
          checked
            ? "translate-x-3 bg-primary-foreground"
            : "translate-x-0.5 bg-foreground/70",
        )}
      />
    </button>
  );
});
