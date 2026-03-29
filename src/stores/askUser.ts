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

export const askUserAtom = atom<AskUserState | null>(null);
