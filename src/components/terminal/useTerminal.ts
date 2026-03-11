import { useRef, useEffect, useCallback } from "react";
import { useSetAtom } from "jotai";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { invoke, listen, generateId } from "@/lib/tauri";
import { terminalIdAtom } from "@/stores/session";
import type { PtyOutput } from "@/lib/types";

interface UseTerminalOptions {
  cwd?: string;
}

export function useTerminal(options: UseTerminalOptions = {}) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const terminalIdRef = useRef<string>(generateId("pty"));
  const setTerminalId = useSetAtom(terminalIdAtom);

  const fit = useCallback(() => {
    fitAddonRef.current?.fit();
  }, []);

  useEffect(() => {
    const container = terminalRef.current;
    if (!container) return;

    const term = new Terminal({
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
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    term.open(container);

    // Suppress dead key events (tildes, accents) to prevent duplicate input on Linux/WebKitGTK
    term.attachCustomKeyEventHandler((event) => {
      if (event.key === "Dead") return false;
      return true;
    });

    try {
      const webglAddon = new WebglAddon();
      webglAddon.onContextLoss(() => {
        webglAddon.dispose();
      });
      term.loadAddon(webglAddon);
    } catch {
      // WebGL not available, canvas renderer is fine
    }

    fitAddon.fit();
    termRef.current = term;
    fitAddonRef.current = fitAddon;

    const ptyId = terminalIdRef.current;
    setTerminalId(ptyId);
    const { cols, rows } = term;

    invoke("pty_create", {
      id: ptyId,
      cwd: options.cwd ?? null,
      cols,
      rows,
    }).catch((err: unknown) => {
      term.writeln(`\r\nFailed to create PTY: ${String(err)}\r\n`);
    });

    const unlistenPromise = listen<PtyOutput>("pty:output", (payload) => {
      if (payload.id === ptyId) {
        term.write(new Uint8Array(payload.data));
      }
    });

    const dataDisposable = term.onData((data) => {
      invoke("pty_write", { id: ptyId, data }).catch((err: unknown) => {
        console.error("pty_write failed:", err);
      });
    });

    const resizeDisposable = term.onResize(({ cols, rows }) => {
      invoke("pty_resize", { id: ptyId, cols, rows }).catch(() => {});
    });

    return () => {
      dataDisposable.dispose();
      resizeDisposable.dispose();
      unlistenPromise.then((unlisten) => unlisten());
      invoke("pty_kill", { id: ptyId }).catch(() => {});
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  }, [options.cwd]);

  return { terminalRef, terminalId: terminalIdRef.current, fit };
}
