import { useAtomValue } from "jotai";
import { toastsAtom } from "@/stores/toast";

const TYPE_COLORS = {
  success: "bg-[#22c55e] text-white",
  error: "bg-[#ef4444] text-white",
  info: "bg-[#1c1c1e] text-[#ededef] border border-white/10",
} as const;

export function Toasts() {
  const toasts = useAtomValue(toastsAtom);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-12 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`pointer-events-auto rounded-md px-4 py-2.5 text-xs shadow-lg ${TYPE_COLORS[toast.type]}`}
        >
          {toast.message}
        </div>
      ))}
    </div>
  );
}
