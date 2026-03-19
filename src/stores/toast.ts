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
