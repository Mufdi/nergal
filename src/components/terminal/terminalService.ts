import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke, listen } from "@/lib/tauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { PtyOutput } from "@/lib/types";
import "@xterm/xterm/css/xterm.css";

// ── Types ──

interface TerminalEntry {
  term: Terminal;
  fitAddon: FitAddon;
  ptyId: string;
  container: HTMLDivElement;
  unlisten: UnlistenFn;
  dataDisposable: { dispose(): void };
  resizeDisposable: { dispose(): void };
}

// ── Module state — lives outside React lifecycle ──

const terminals = new Map<string, TerminalEntry>();
const pending = new Set<string>();
let hostElement: HTMLDivElement | null = null;
let activeId: string | null = null;

// ── Constants ──

const TERM_OPTIONS: ConstructorParameters<typeof Terminal>[0] = {
  cursorBlink: true,
  fontSize: 13,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  theme: {
    background: "#141415",
    foreground: "#ededef",
    cursor: "#f97316",
    cursorAccent: "#141415",
    selectionBackground: "#f9731633",
    selectionForeground: "#ededef",
    black: "#141415",
    red: "#ef4444",
    green: "#22c55e",
    yellow: "#eab308",
    blue: "#3b82f6",
    magenta: "#a855f7",
    cyan: "#06b6d4",
    white: "#ededef",
    brightBlack: "#5c5c5f",
    brightRed: "#f87171",
    brightGreen: "#4ade80",
    brightYellow: "#facc15",
    brightBlue: "#60a5fa",
    brightMagenta: "#c084fc",
    brightCyan: "#22d3ee",
    brightWhite: "#ffffff",
  },
  allowProposedApi: true,
};

// ── Private helpers ──

function afterLayout(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

function wireIMEFix(term: Terminal, container: HTMLElement, ptyId: string) {
  let composing = false;
  let suppressUntil = 0;
  const textarea = container.querySelector(".xterm-helper-textarea") as HTMLTextAreaElement | null;

  if (textarea) {
    textarea.addEventListener("compositionstart", () => { composing = true; });
    textarea.addEventListener("compositionupdate", (e) => { e.stopPropagation(); });
    textarea.addEventListener("compositionend", (e) => {
      composing = false;
      suppressUntil = Date.now() + 150;
      const composed = (e as CompositionEvent).data;
      if (composed) invoke("pty_write", { id: ptyId, data: composed }).catch(() => {});
    });
  }

  term.attachCustomKeyEventHandler((event) => {
    if (event.key === "Dead" || composing || Date.now() < suppressUntil) return false;
    return true;
  });

  return { isComposing: () => composing, isSuppressed: () => Date.now() < suppressUntil };
}

// ── Public API ──

export function setHost(el: HTMLDivElement | null): void {
  hostElement = el;
}

/// Show the given session's terminal. Creates it if it doesn't exist.
export async function show(sessionId: string, cwd: string, mode: "new" | "continue" | "resume_pick" = "new"): Promise<void> {
  activeId = sessionId;

  // Toggle visibility + force redraw on the shown terminal
  for (const [id, entry] of terminals) {
    if (id === sessionId) {
      entry.container.style.display = "flex";
    } else {
      entry.container.style.display = "none";
    }
  }

  // Already attached — fit + refresh to redraw after display:none→flex
  if (terminals.has(sessionId)) {
    const entry = terminals.get(sessionId)!;
    requestAnimationFrame(() => {
      entry.fitAddon.fit();
      entry.term.refresh(0, entry.term.rows - 1);
      entry.term.focus();
    });
    return;
  }

  // Already being created
  if (pending.has(sessionId)) return;
  if (!hostElement) return;

  pending.add(sessionId);

  try {
    const container = document.createElement("div");
    container.style.cssText = "position:absolute;inset:0;display:flex;overflow:hidden;background:#141415;";
    hostElement.appendChild(container);

    // Wait for browser layout so container has real dimensions
    await afterLayout();

    const term = new Terminal(TERM_OPTIONS);
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);

    // No WebGL/Canvas addon — DOM renderer is sufficient and has no context limits
    fitAddon.fit();

    // Backend: createOrAttach — PTY + shell ready + claude
    const { pty_id: ptyId } = await invoke<{ pty_id: string }>("start_claude_session", {
      sessionId,
      cwd,
      cols: term.cols,
      rows: term.rows,
      resume: mode === "new" ? null : mode,
    });

    const ime = wireIMEFix(term, container, ptyId);

    const unlisten = await listen<PtyOutput>("pty:output", (payload) => {
      if (payload.id === ptyId) term.write(new Uint8Array(payload.data));
    });

    const dataDisposable = term.onData((data) => {
      if (ime.isComposing() || ime.isSuppressed()) return;
      invoke("pty_write", { id: ptyId, data }).catch((err: unknown) => console.error("pty_write:", err));
    });

    const resizeDisposable = term.onResize(({ cols, rows }) => {
      invoke("pty_resize", { id: ptyId, cols, rows }).catch(() => {});
    });

    terminals.set(sessionId, { term, fitAddon, ptyId, container, unlisten, dataDisposable, resizeDisposable });
    term.focus();

    if (activeId !== sessionId) {
      container.style.display = "none";
    }
  } catch (err) {
    console.error("Failed to create terminal for session", sessionId, err);
  } finally {
    pending.delete(sessionId);
  }
}

export function destroy(sessionId: string): void {
  const entry = terminals.get(sessionId);
  if (!entry) return;

  entry.dataDisposable.dispose();
  entry.resizeDisposable.dispose();
  entry.unlisten();
  entry.term.dispose();
  entry.container.remove();
  terminals.delete(sessionId);

  invoke("kill_session_pty", { sessionId }).catch(() => {});
}

export async function writeToSession(sessionId: string, text: string): Promise<void> {
  const entry = terminals.get(sessionId);
  if (!entry) return;
  await invoke("write_to_session_pty", { sessionId, data: text });
}

export function hasTerminal(sessionId: string): boolean {
  return terminals.has(sessionId);
}

export function fitActive(): void {
  if (!activeId) return;
  const entry = terminals.get(activeId);
  if (!entry) return;
  entry.fitAddon.fit();
  entry.term.refresh(0, entry.term.rows - 1);
}
