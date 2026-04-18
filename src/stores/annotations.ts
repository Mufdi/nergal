import { atom, type Getter } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { activePlanReviewStatusAtom } from "./plan";
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

/// Where the current annotation batch lives. Plan scope persists to SQLite and
/// is delivered via plan-review FIFO. Spec scope is in-memory only (MVP) and
/// delivered via the inject-edits UserPromptSubmit hook.
export type AnnotationScope =
  | { kind: "plan"; sessionId: string }
  | { kind: "spec"; specPath: string };

export const annotationScopeAtom = atom<AnnotationScope | null>(null);

export const annotationModeAtom = atom(false);

export const canEnterAnnotationModeAtom = atom((get) => {
  return get(activePlanReviewStatusAtom) === "pending_review";
});

export const annotationMapAtom = atom<Record<string, Annotation[]>>({});
export const specAnnotationMapAtom = atom<Record<string, Annotation[]>>({});

function resolveScope(get: Getter): AnnotationScope | null {
  const explicit = get(annotationScopeAtom);
  if (explicit) return explicit;
  const sid = get(activeSessionIdAtom);
  return sid ? { kind: "plan", sessionId: sid } : null;
}

export const activeAnnotationsAtom = atom<Annotation[]>((get) => {
  const scope = resolveScope(get);
  if (!scope) return [];
  if (scope.kind === "plan") return get(annotationMapAtom)[scope.sessionId] ?? [];
  return get(specAnnotationMapAtom)[scope.specPath] ?? [];
});

export const addAnnotationAtom = atom(
  null,
  (get, set, annotation: Omit<Annotation, "id"> & { highlightId?: string }) => {
    const scope = resolveScope(get);
    if (!scope) return;
    const id = annotation.highlightId ?? `ann-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const entry = { ...annotation, id };

    if (scope.kind === "plan") {
      set(annotationMapAtom, (prev) => {
        const existing = prev[scope.sessionId] ?? [];
        return { ...prev, [scope.sessionId]: [...existing, entry] };
      });
      invoke("save_annotation", {
        id,
        sessionId: scope.sessionId,
        annType: annotation.type,
        target: annotation.target,
        content: annotation.content,
        startMeta: JSON.stringify(annotation.startMeta),
        endMeta: JSON.stringify(annotation.endMeta),
      }).catch(console.error);
      return;
    }

    set(specAnnotationMapAtom, (prev) => {
      const existing = prev[scope.specPath] ?? [];
      return { ...prev, [scope.specPath]: [...existing, entry] };
    });
    invoke("save_spec_annotation", {
      id,
      specKey: scope.specPath,
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
    const scope = resolveScope(get);
    if (!scope) return;
    if (scope.kind === "plan") {
      set(annotationMapAtom, (prev) => {
        const existing = prev[scope.sessionId] ?? [];
        return { ...prev, [scope.sessionId]: existing.filter((a) => a.id !== annotationId) };
      });
      invoke("delete_annotation", { id: annotationId }).catch(console.error);
      return;
    }
    set(specAnnotationMapAtom, (prev) => {
      const existing = prev[scope.specPath] ?? [];
      return { ...prev, [scope.specPath]: existing.filter((a) => a.id !== annotationId) };
    });
    invoke("delete_spec_annotation", { id: annotationId }).catch(console.error);
  },
);

export const clearAnnotationsAtom = atom(
  null,
  (get, set) => {
    const scope = resolveScope(get);
    if (!scope) return;
    if (scope.kind === "plan") {
      set(annotationMapAtom, (prev) => ({ ...prev, [scope.sessionId]: [] }));
      invoke("clear_annotations", { sessionId: scope.sessionId }).catch(console.error);
      return;
    }
    set(specAnnotationMapAtom, (prev) => ({ ...prev, [scope.specPath]: [] }));
    invoke("clear_spec_annotations", { specKey: scope.specPath }).catch(console.error);
  },
);

export function serializeAnnotations(annotations: Annotation[], targetPath: string): string {
  if (annotations.length === 0) return "";

  const lines = [`Re-read plan at ${targetPath}. Address these annotations:`];
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

export interface SpecFeedbackContext {
  changeName: string;
  artifactPath: string;
  isMaster: boolean;
}

/// Build the repo-relative path that Claude can pass to the Read tool.
function repoPathFor(ctx: SpecFeedbackContext): string {
  if (ctx.isMaster) return `openspec/${ctx.artifactPath}`;
  return `openspec/changes/${ctx.changeName}/${ctx.artifactPath}`;
}

export function serializeSpecAnnotations(
  annotations: Annotation[],
  ctx: SpecFeedbackContext,
): string {
  if (annotations.length === 0) return "";

  const repoPath = repoPathFor(ctx);
  const header = ctx.isMaster
    ? `Review the OpenSpec capability spec and address my annotations.`
    : `Review the OpenSpec change artifact and address my annotations.`;

  const lines = [
    header,
    `- Change: ${ctx.isMaster ? "(master specs)" : ctx.changeName}`,
    `- File: ${repoPath}`,
    "",
    "Annotations:",
  ];
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
