import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { invoke } from "@/lib/tauri";
import type { DomMeta } from "@/lib/highlighter";

export type AnnotationType = "comment" | "replace" | "delete" | "insert";

export interface Annotation {
  id: string;
  type: AnnotationType;
  target: string;
  content: string;
  startMeta: DomMeta;
  endMeta: DomMeta;
}

export const annotationMapAtom = atom<Record<string, Annotation[]>>({});

export const activeAnnotationsAtom = atom<Annotation[]>((get) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return [];
  return get(annotationMapAtom)[sessionId] ?? [];
});

export const addAnnotationAtom = atom(
  null,
  (get, set, annotation: Omit<Annotation, "id"> & { highlightId?: string }) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    const id = annotation.highlightId ?? `ann-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    set(annotationMapAtom, (prev) => {
      const existing = prev[sessionId] ?? [];
      return { ...prev, [sessionId]: [...existing, { ...annotation, id }] };
    });
    invoke("save_annotation", {
      id,
      sessionId,
      annType: annotation.type,
      target: annotation.target,
      content: annotation.content,
      startMeta: JSON.stringify(annotation.startMeta),
      endMeta: JSON.stringify(annotation.endMeta),
    }).catch(console.error);
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
    invoke("delete_annotation", { id: annotationId }).catch(console.error);
  },
);

export const clearAnnotationsAtom = atom(
  null,
  (get, set) => {
    const sessionId = get(activeSessionIdAtom);
    if (!sessionId) return;
    set(annotationMapAtom, (prev) => ({ ...prev, [sessionId]: [] }));
    invoke("clear_annotations", { sessionId }).catch(console.error);
  },
);

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
