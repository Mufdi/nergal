import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";

export type AnnotationType = "comment" | "replace" | "delete" | "insert";

export interface Annotation {
  id: string;
  type: AnnotationType;
  target: string;
  content: string;
  position: { start: number; end: number };
}

export const annotationMapAtom = atom<Record<string, Annotation[]>>({});

export const activeAnnotationsAtom = atom<Annotation[]>((get) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return [];
  return get(annotationMapAtom)[sessionId] ?? [];
});

export const addAnnotationAtom = atom(
  null,
  (get, set, annotation: Omit<Annotation, "id">) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    const id = `ann-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    set(annotationMapAtom, (prev) => {
      const existing = prev[sessionId] ?? [];
      return { ...prev, [sessionId]: [...existing, { ...annotation, id }] };
    });
  },
);

export const removeAnnotationAtom = atom(
  null,
  (get, set, annotationId: string) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    set(annotationMapAtom, (prev) => {
      const existing = prev[sessionId] ?? [];
      return { ...prev, [sessionId]: existing.filter((a) => a.id !== annotationId) };
    });
  },
);

export const clearAnnotationsAtom = atom(
  null,
  (get, set) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    set(annotationMapAtom, (prev) => ({ ...prev, [sessionId]: [] }));
  },
);

/// Serialize annotations into structured feedback for the UserPromptSubmit hook.
export function serializeAnnotations(annotations: Annotation[], planPath: string): string {
  if (annotations.length === 0) return "";

  const lines = [`Re-read plan at ${planPath}. Address these annotations:`];
  for (let i = 0; i < annotations.length; i++) {
    const a = annotations[i];
    switch (a.type) {
      case "delete":
        lines.push(`[${i + 1}] DELETE section "${a.target}" — reason: ${a.content}`);
        break;
      case "replace":
        lines.push(`[${i + 1}] REPLACE "${a.target}" with "${a.content}"`);
        break;
      case "comment":
        lines.push(`[${i + 1}] COMMENT on "${a.target}": ${a.content}`);
        break;
      case "insert":
        lines.push(`[${i + 1}] INSERT after "${a.target}": ${a.content}`);
        break;
    }
  }
  return lines.join("\n");
}
