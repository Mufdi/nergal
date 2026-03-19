import { atom } from "jotai";
import { sileo } from "sileo";

export interface Toast {
  message: string;
  type: "success" | "error" | "info";
}

export const toastsAtom = atom(
  () => [],
  (_get, _set, toast: Toast) => {
    switch (toast.type) {
      case "success":
        sileo.success({ title: toast.message });
        break;
      case "error":
        sileo.error({ title: toast.message });
        break;
      case "info":
        sileo.info({ title: toast.message });
        break;
    }
  },
);
