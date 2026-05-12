import { atom } from "jotai";

export interface AskUserQuestion {
  question: string;
  header: string;
  options: string[];
  multi_select: boolean;
}

export interface AskUserState {
  questions: AskUserQuestion[];
  decisionPath: string;
  sessionId: string;
}

// Retained as a stub to keep the now-hidden AskUserModal and SessionIndicator
// imports compiling. CC's TUI owns the question UX; cluihud only signals
// which session is waiting via `pendingAsksAtom` below.
export const askUserAtom = atom<AskUserState | null>(null);

export const pendingAsksAtom = atom<Record<string, true>>({});
