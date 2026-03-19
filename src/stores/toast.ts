import { atom } from "jotai";
import { sileo } from "sileo";

export interface Toast {
  message: string;
  description?: string;
  type: "success" | "error" | "info";
}

const DARK_STYLES = {
  title: "text-white!",
  description: "text-white/75!",
};

const STATE_BORDERS: Record<Toast["type"], string> = {
  success: "border border-green-500/40!",
  error: "border border-red-500/40!",
  info: "border border-blue-500/40!",
};

export const toastsAtom = atom(
  () => [],
  (_get, _set, toast: Toast) => {
    const opts = {
      title: toast.message,
      description: toast.description,
      fill: "#141415",
      styles: {
        ...DARK_STYLES,
        badge: STATE_BORDERS[toast.type],
      },
    };
    switch (toast.type) {
      case "success":
        sileo.success(opts);
        break;
      case "error":
        sileo.error(opts);
        break;
      case "info":
        sileo.info(opts);
        break;
    }
  },
);
