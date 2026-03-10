import { useAtomValue } from "jotai";
import { toastsAtom } from "@/stores/toast";

const TYPE_COLORS = {
  success: "bg-success/90 text-white",
  error: "bg-danger/90 text-white",
  info: "bg-surface-overlay text-text",
} as const;

export function Toasts() {
  const toasts = useAtomValue(toastsAtom);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-12 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`pointer-events-auto px-4 py-2 text-xs shadow-lg ${TYPE_COLORS[toast.type]}`}
        >
          {toast.message}
        </div>
      ))}
    </div>
  );
}
