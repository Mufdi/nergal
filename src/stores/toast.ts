import { atom } from "jotai";

export interface Toast {
  id: string;
  message: string;
  type: "success" | "error" | "info";
}

const toastListAtom = atom<Toast[]>([]);

export const toastsAtom = atom(
  (get) => get(toastListAtom),
  (_get, set, toast: Omit<Toast, "id">) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    set(toastListAtom, (prev) => [...prev, { ...toast, id }]);
    setTimeout(() => {
      set(toastListAtom, (prev) => prev.filter((t) => t.id !== id));
    }, 3000);
  },
);
