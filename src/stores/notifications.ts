import { atom } from "jotai";

export interface NotificationEntry {
  id: string;
  message: string;
  description?: string;
  type: "success" | "error" | "info";
  ts: number;
}

const MAX_HISTORY = 100;

export const notificationHistoryAtom = atom<NotificationEntry[]>([]);
export const lastSeenNotificationTsAtom = atom<number>(0);

/// Unread = entries newer than the last time the user opened the center.
export const unreadNotificationsAtom = atom((get) => {
  const lastSeen = get(lastSeenNotificationTsAtom);
  return get(notificationHistoryAtom).filter((n) => n.ts > lastSeen).length;
});

let seq = 0;

/// Mirror of every toast — fed by the toast writer so the center is a complete
/// log of what flashed by. Newest first, capped so a long session can't grow it
/// unbounded.
export const pushNotificationAction = atom(
  null,
  (get, set, entry: Omit<NotificationEntry, "id" | "ts">) => {
    seq += 1;
    const next: NotificationEntry = { ...entry, id: `n${Date.now()}-${seq}`, ts: Date.now() };
    set(notificationHistoryAtom, [next, ...get(notificationHistoryAtom)].slice(0, MAX_HISTORY));
  },
);

export const markNotificationsSeenAtom = atom(null, (_get, set) => {
  set(lastSeenNotificationTsAtom, Date.now());
});

export const clearNotificationsAtom = atom(null, (_get, set) => {
  set(notificationHistoryAtom, []);
});
