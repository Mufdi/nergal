import { useEffect, useState } from "react";

/**
 * Returns true for `durationMs` after `focused` flips from false → true,
 * then resolves back to false. While `focused` stays false, returns false.
 * Used by panel containers to render a brief accent-color flash on focus
 * change (legacy "pulse" focus mode) before the border fades back to the
 * resting border color via `transition-[border-color]`.
 */
export function useFocusPulse(focused: boolean, durationMs = 300): boolean {
  const [pulsing, setPulsing] = useState(false);

  useEffect(() => {
    if (!focused) {
      setPulsing(false);
      return;
    }
    setPulsing(true);
    const timer = setTimeout(() => setPulsing(false), durationMs);
    return () => clearTimeout(timer);
  }, [focused, durationMs]);

  return pulsing;
}
