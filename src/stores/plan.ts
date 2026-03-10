import { atom } from "jotai";
import type { PlanMode, DiffLine } from "@/lib/types";

export const planContentAtom = atom<string>("");
export const planModeAtom = atom<PlanMode>("view");
export const planOriginalAtom = atom<string>("");
export const planPathAtom = atom<string>("");
export const planDiffAtom = atom<DiffLine[]>([]);
export const planVisibleAtom = atom<boolean>(false);
