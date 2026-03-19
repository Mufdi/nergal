import { atom } from "jotai";
import { sileo } from "sileo";

export interface Toast {
  message: string;
  description?: string;
  type: "success" | "error" | "info";
}

export const toastsAtom = atom(
  () => [],
  (_get, _set, toast: Toast) => {
    const opts = {
      title: toast.message,
      description: toast.description,
      fill: "#171717",
      styles: {
        title: "text-white! text-xs!",
        description: "text-white/75! text-[11px]!",
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
