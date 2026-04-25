import { atom } from "jotai";

export interface ShipDialogState {
  open: boolean;
  sessionId: string | null;
  inlineMessage: string | null;
}

export const shipDialogAtom = atom<ShipDialogState>({
  open: false,
  sessionId: null,
  inlineMessage: null,
});

export const triggerShipAtom = atom<{ tick: number; sessionId: string | null; inlineMessage: string | null }>({
  tick: 0,
  sessionId: null,
  inlineMessage: null,
});

export type ShipProgressStage = "commit" | "push" | "pr";

export interface ShipProgressEvent {
  session_id: string;
  stage: ShipProgressStage;
  ok: boolean;
}

export const shipProgressAtom = atom<ShipProgressEvent | null>(null);
