import { atom } from "jotai";
import { sileo } from "sileo";
import { pushNotificationAction } from "./notifications";

export interface Toast {
  message: string;
  description?: string;
  type: "success" | "error" | "info";
  /// Optional action button. When present the toast renders via sileo.action
  /// (the only variant with a button) regardless of `type`.
  action?: { label: string; onClick: () => void };
}

export const toastsAtom = atom(
  () => [],
  (_get, set, toast: Toast) => {
    const opts = {
      title: toast.message,
      description: toast.description,
      fill: "#171717",
    };
    if (toast.action) {
      const { label, onClick } = toast.action;
      const id = sileo.action({
        ...opts,
        button: {
          title: label,
          onClick: () => {
            sileo.dismiss(id);
            onClick();
          },
        },
      });
    } else {
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
    }
    // Mirror into the notification center so toasts are re-readable later.
    set(pushNotificationAction, {
      message: toast.message,
      description: toast.description,
      type: toast.type,
    });
  },
);
