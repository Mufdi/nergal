/// Promise-based confirmation dialog. `confirm()` resolves to `true` when the
/// user confirms. Rendering is handled by the single `<ConfirmHost/>` mounted
/// in Workspace; this module is the imperative bridge so Jotai atoms and plain
/// async functions can `await confirm(...)` without wiring a component.

export interface ConfirmOptions {
  title: string;
  /// Rendered as HTML. Callers MUST escape any user-controlled substring
  /// (task/workspace/session names) before interpolating — use `escapeHtml`
  /// from `@/lib/escapeHtml` (the single shared implementation).
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /// Retained for call-site compatibility; the dialog is iconless (matches the
  /// branch-rename mini-modal), so this no longer drives any visual.
  kind?: "warning" | "error" | "question";
  destructive?: boolean;
  /// Safety property for confirmations triggered by external input (e.g. a
  /// deep link). When `false`, Enter does NOT proceed — it cancels instead,
  /// so a stray Enter while the user is typing elsewhere (attacker-
  /// controlled timing) can't confirm; proceeding then requires an explicit
  /// pointer/keyed action on the proceed button. Defaults to `true` (today's
  /// behavior) — every existing caller is unaffected.
  enterConfirms?: boolean;
}

export interface ActiveConfirm {
  opts: ConfirmOptions;
  resolve: (confirmed: boolean) => void;
}

/// Pure decision for what an Enter keypress does inside `ConfirmHost`'s
/// capture-phase keydown handler. Extracted so the anti-stray-Enter property
/// (confirm-deep-link-session-spawn D5) is unit-testable without rendering
/// the dialog: Enter proceeds unless the caller opted into `enterConfirms:
/// false` (external-input-triggered confirms).
export function enterKeyConfirms(opts: ConfirmOptions): boolean {
  return opts.enterConfirms !== false;
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
