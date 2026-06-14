import { atom } from "jotai";

export interface NotificationEntry {
  id: string;
  message: string;
  description?: string;
  type: "success" | "error" | "info";
  ts: number;
}

const MAX_HISTORY = 100;

/// In-memory log of every toast — a passive history the user consults on
/// demand, not an unread queue. Cleared on app restart (toasts are ephemeral
/// status messages; losing them on close is fine). Newest first, capped.
export const notificationHistoryAtom = atom<NotificationEntry[]>([]);

let seq = 0;

export const pushNotificationAction = atom(
  null,
  (get, set, entry: Omit<NotificationEntry, "id" | "ts">) => {
    seq += 1;
    const next: NotificationEntry = { ...entry, id: `n${Date.now()}-${seq}`, ts: Date.now() };
    set(notificationHistoryAtom, [next, ...get(notificationHistoryAtom)].slice(0, MAX_HISTORY));
  },
);

export const clearNotificationsAtom = atom(null, (_get, set) => {
  set(notificationHistoryAtom, []);
});
