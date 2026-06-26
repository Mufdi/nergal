/// Promise-based confirmation dialog. `confirm()` resolves to `true` when the
/// user confirms. Rendering is handled by the single `<ConfirmHost/>` mounted
/// in Workspace; this module is the imperative bridge so Jotai atoms and plain
/// async functions can `await confirm(...)` without wiring a component.

export interface ConfirmOptions {
  title: string;
  /// Rendered as HTML. Callers MUST escape any user-controlled substring
  /// (task/workspace/session names) before interpolating — see `escapeHtml`
  /// in stores/clickup.ts and stores/linear.ts.
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /// Retained for call-site compatibility; the dialog is iconless (matches the
  /// branch-rename mini-modal), so this no longer drives any visual.
  kind?: "warning" | "error" | "question";
  destructive?: boolean;
}

export interface ActiveConfirm {
  opts: ConfirmOptions;
  resolve: (confirmed: boolean) => void;
}

let active: ActiveConfirm | null = null;
const queue: ActiveConfirm[] = [];
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function subscribeConfirm(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getActiveConfirm(): ActiveConfirm | null {
  return active;
}

export function confirm(opts: ConfirmOptions): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const entry: ActiveConfirm = { opts, resolve };
    if (active) {
      queue.push(entry);
    } else {
      active = entry;
      emit();
    }
  });
}

/// Resolve the active confirm and advance the queue. Called by `<ConfirmHost/>`.
/// Advancing `active` before resolving keeps the store consistent if the
/// awaiting caller chains another `confirm()` in its continuation.
export function resolveConfirm(confirmed: boolean): void {
  const current = active;
  if (!current) return;
  active = queue.shift() ?? null;
  emit();
  current.resolve(confirmed);
}
