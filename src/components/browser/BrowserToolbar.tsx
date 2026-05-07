import { useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  ArrowLeft,
  ArrowRight,
  RotateCw,
  ExternalLink,
  Minimize2,
  Globe,
  Sun,
  Moon,
} from "lucide-react";
import {
  browserActiveTabAtom,
  browserColorSchemeForSessionAtom,
  browserGoBackAction,
  browserGoForwardAction,
  browserModeForSessionAtom,
  browserNavigateAction,
  browserNewTabAction,
  browserReloadAction,
  browserSetColorSchemeAction,
  browserSetModeAction,
  localhostPortsAtom,
} from "@/stores/browser";

interface Props {
  sessionId: string;
  /// Surfaced so the parent can register an imperative focus handle (used by
  /// the Cmd+L shortcut wired in stores/shortcuts.ts).
  inputRef?: React.RefObject<HTMLInputElement | null>;
}

export function BrowserToolbar({ sessionId, inputRef }: Props) {
  const activeTab = useAtomValue(browserActiveTabAtom);
  const mode = useAtomValue(browserModeForSessionAtom);
  const colorScheme = useAtomValue(browserColorSchemeForSessionAtom);
  const ports = useAtomValue(localhostPortsAtom);
  const navigate = useSetAtom(browserNavigateAction);
  const newTab = useSetAtom(browserNewTabAction);
  const goBack = useSetAtom(browserGoBackAction);
  const goForward = useSetAtom(browserGoForwardAction);
  const reload = useSetAtom(browserReloadAction);
  const setMode = useSetAtom(browserSetModeAction);
  const setColorScheme = useSetAtom(browserSetColorSchemeAction);

  const url = activeTab?.url ?? "";
  const tabId = activeTab?.id ?? null;

  const [draft, setDraft] = useState(url);
  const [error, setError] = useState<string | null>(null);
  const [portMenuOpen, setPortMenuOpen] = useState(false);
  const internalRef = useRef<HTMLInputElement | null>(null);
  const ref = inputRef ?? internalRef;
  const portMenuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setDraft(url);
    setError(null);
  }, [url]);

  useEffect(() => {
    function focusUrl() {
      const node = ref.current;
      if (!node) return;
      node.focus();
      node.select();
    }
    document.addEventListener("cluihud:browser-focus-url", focusUrl);
    return () => document.removeEventListener("cluihud:browser-focus-url", focusUrl);
  }, [ref]);

  useEffect(() => {
    if (!portMenuOpen) return;
    function close(e: MouseEvent) {
      if (!portMenuRef.current?.contains(e.target as Node)) setPortMenuOpen(false);
    }
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [portMenuOpen]);

  async function commit() {
    const raw = draft.trim();
    if (!raw) return;
    const next = normalizeUrl(raw);
    try {
      if (!tabId) {
        await newTab({ sessionId, url: next });
      } else {
        await navigate({ sessionId, url: next, tabId });
      }
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function selectPort(port: number) {
    setPortMenuOpen(false);
    try {
      await newTab({ sessionId, url: `http://localhost:${port}` });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  const activePort = matchLocalhostPort(url);

  return (
    <div className="flex items-center gap-1 border-b border-border/60 bg-card/40 px-2 py-1.5">
      <ToolbarButton
        title="Back"
        onClick={() => tabId && goBack({ sessionId, tabId })}
        disabled={!tabId || (activeTab?.back.length ?? 0) === 0}
      >
        <ArrowLeft size={14} />
      </ToolbarButton>
      <ToolbarButton
        title="Forward"
        onClick={() => tabId && goForward({ sessionId, tabId })}
        disabled={!tabId || (activeTab?.forward.length ?? 0) === 0}
      >
        <ArrowRight size={14} />
      </ToolbarButton>
      <ToolbarButton
        title="Reload"
        onClick={() => tabId && reload({ sessionId, tabId })}
        disabled={!tabId}
      >
        <RotateCw size={14} />
      </ToolbarButton>
      <div ref={portMenuRef} className="relative">
        <ToolbarButton
          title={
            ports.length === 0
              ? "No localhost dev servers detected"
              : `Detected ports: ${ports.join(", ")}`
          }
          onClick={() => setPortMenuOpen((v) => !v)}
        >
          <Globe size={14} className={ports.length > 0 ? "text-primary/80" : undefined} />
        </ToolbarButton>
        {portMenuOpen && (
          <div className="absolute left-0 top-full z-50 mt-1 min-w-44 rounded-md border border-border/60 bg-popover p-1 shadow-md">
            {ports.length === 0 ? (
              <div className="px-2 py-1.5 text-[11px] text-muted-foreground">
                No localhost dev servers detected.
              </div>
            ) : (
              <>
                <div className="px-2 pb-1 pt-1.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">
                  Detected
                </div>
                {ports.map((port) => {
                  const isActive = port === activePort;
                  return (
                    <button
                      key={port}
                      type="button"
                      onClick={() => selectPort(port)}
                      className={`flex w-full items-center justify-between gap-3 rounded px-2 py-1 text-left font-mono text-xs transition-colors hover:bg-secondary ${
                        isActive ? "bg-secondary/60 text-foreground" : "text-muted-foreground"
                      }`}
                    >
                      <span>localhost:{port}</span>
                      {isActive && (
                        <span className="text-[9px] uppercase tracking-wider text-primary">
                          current
                        </span>
                      )}
                    </button>
                  );
                })}
              </>
            )}
          </div>
        )}
      </div>
      <input
        ref={ref}
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          if (error) setError(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
        spellCheck={false}
        placeholder="http://localhost:5173"
        className={`mx-1 h-7 flex-1 rounded-md border bg-background px-2 text-xs font-mono outline-none ring-1 ring-transparent transition focus:ring-primary/40 ${
          error ? "border-destructive/60" : "border-border/60"
        }`}
        title={error ?? undefined}
      />
      <ToolbarButton
        title={colorScheme === "dark" ? "Switch to light" : "Switch to dark"}
        onClick={() =>
          setColorScheme({
            sessionId,
            scheme: colorScheme === "dark" ? "light" : "dark",
          })
        }
      >
        {colorScheme === "dark" ? <Sun size={14} /> : <Moon size={14} />}
      </ToolbarButton>
      <ToolbarButton
        title={mode === "dock" ? "Pop out to floating window" : "Return to dock"}
        onClick={() =>
          setMode({ sessionId, mode: mode === "dock" ? "floating" : "dock" })
        }
      >
        {mode === "dock" ? <ExternalLink size={14} /> : <Minimize2 size={14} />}
      </ToolbarButton>
    </div>
  );
}

/// Lenient parser for URL bar input. Users typically type
/// "localhost:5173" or "5173" without the scheme; the backend's strict
/// scheme validator would reject those. Normalize to a valid URL before
/// dispatching, so the URL bar feels like a real browser's.
function normalizeUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return trimmed;
  if (trimmed === "about:blank") return trimmed;
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  // Bare port like "5173" or ":5173" → http://localhost:<port>
  const portOnly = trimmed.replace(/^:/, "");
  if (/^\d{2,5}$/.test(portOnly)) return `http://localhost:${portOnly}`;
  // host:port or host alone → prepend http://
  return `http://${trimmed}`;
}

/// Returns the port number iff the URL is a localhost (or 127.0.0.1) URL,
/// so the selector can highlight the currently-active port.
function matchLocalhostPort(url: string): number | null {
  try {
    const u = new URL(url);
    if (u.hostname !== "localhost" && u.hostname !== "127.0.0.1") return null;
    const port = u.port ? Number.parseInt(u.port, 10) : null;
    return Number.isFinite(port) ? port : null;
  } catch {
    return null;
  }
}

function ToolbarButton({
  children,
  onClick,
  disabled,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  title: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={title}
      className="grid h-7 w-7 place-items-center rounded-md text-muted-foreground transition hover:bg-secondary/60 hover:text-foreground disabled:pointer-events-none disabled:opacity-30"
    >
      {children}
    </button>
  );
}
