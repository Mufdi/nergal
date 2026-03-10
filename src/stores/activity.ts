import { atom } from "jotai";
import type { ActivityEntry } from "@/lib/types";

const MAX_ENTRIES = 200;

const rawActivityAtom = atom<ActivityEntry[]>([]);

export const activityAtom = atom(
  (get) => get(rawActivityAtom),
  (_get, set, entry: ActivityEntry) => {
    set(rawActivityAtom, (prev) => {
      const next = [...prev, entry];
      if (next.length > MAX_ENTRIES) {
        return next.slice(next.length - MAX_ENTRIES);
      }
      return next;
    });
  },
);
