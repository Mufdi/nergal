import { useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Globe } from "lucide-react";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  browserColorSchemeForSessionAtom,
  browserNavigateAction,
  browserNewTabAction,
  browserSessionForActiveAtom,
  localhostPortsAtom,
} from "@/stores/browser";
import { BrowserToolbar } from "./BrowserToolbar";
import { BrowserTabStrip } from "./BrowserTabStrip";

const BLANK_URLS = new Set(["", "about:blank"]);

/// Live-preview browser. Renders an `<iframe>` per tab; only the active
/// tab's iframe is visible (others are `display:none`) so SPA state
/// survives tab switches and mode switches. `color-scheme` is only set on
/// the iframe element so the panel chrome (toolbar, tabs) keeps cluihud's
/// theming and doesn't cascade an inconsistent scheme.
export function BrowserPanel() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const session = useAtomValue(browserSessionForActiveAtom);
  const colorScheme = useAtomValue(browserColorSchemeForSessionAtom);

  if (!sessionId) {
    return <Placeholder text="Select a session first" />;
  }

  return (
    <div className="flex h-full flex-col bg-background">
      <BrowserTabStrip sessionId={sessionId} />
      <BrowserToolbar sessionId={sessionId} />
      <div className="relative min-h-0 flex-1">
        {session.tabs.length === 0 ? (
          <BlankHomepage sessionId={sessionId} />
        ) : (
          session.tabs.map((tab) => {
            const isActive = tab.id === session.activeTabId;
            const isBlank = BLANK_URLS.has(tab.url);
            return (
              <div
                key={tab.id}
                className="absolute inset-0"
                style={{ display: isActive ? undefined : "none" }}
              >
                {isBlank ? (
                  <BlankHomepage sessionId={sessionId} />
                ) : (
                  <iframe
                    key={tab.reloadKey}
                    src={
                      tab.cacheBust > 0
                        ? `${tab.url}${tab.url.includes("?") ? "&" : "?"}_cb=${tab.cacheBust}`
                        : tab.url
                    }
                    sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals"
                    referrerPolicy="no-referrer"
                    className="absolute inset-0 h-full w-full border-0 bg-white"
                    style={{ colorScheme }}
                    title={`Live preview: ${tab.label}`}
                    /// Cross-origin iframes capture keyboard focus on click, so
                    /// parent-window shortcuts (Ctrl+L focus URL bar, Ctrl+Shift+0
                    /// toggle floating, Ctrl+K palette) stop firing while the user
                    /// is interacting inside the page. Blurring on mouseleave
                    /// returns focus to the parent the moment the user moves
                    /// toward the toolbar — the natural motion for invoking a
                    /// shortcut. Click-to-interact still works (re-focuses on
                    /// click).
                    onMouseLeave={(e) => {
                      if (document.activeElement === e.currentTarget) {
                        e.currentTarget.blur();
                        // Re-focus the BrowserHost wrapper so subsequent
                        // shortcuts (Ctrl+T, Ctrl+Tab, Ctrl+L) match the
                        // `target.closest("[data-browser-host]")` gate in
                        // useKeyboardShortcuts. Without this, focus would
                        // fall back to <body> which is not inside the
                        // host, and the browser-focused override would
                        // bail out.
                        const host = e.currentTarget.closest(
                          "[data-browser-host]",
                        ) as HTMLElement | null;
                        host?.focus({ preventScroll: true });
                      }
                    }}
                  />
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

/// Shown when no URL is loaded. Surfaces detected dev-server ports as one-tap
/// buttons and supports arrow-key + Enter navigation.
function BlankHomepage({ sessionId }: { sessionId: string }) {
  const ports = useAtomValue(localhostPortsAtom);
  const session = useAtomValue(browserSessionForActiveAtom);
  const navigate = useSetAtom(browserNavigateAction);
  const newTab = useSetAtom(browserNewTabAction);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [selected, setSelected] = useState(0);

  // Keep selection in range when the port list changes.
  useEffect(() => {
    if (selected >= ports.length) setSelected(Math.max(0, ports.length - 1));
  }, [ports.length, selected]);

  // Auto-focus the container so arrow keys work without a manual click.
  // Also re-focuses every time the host transitions from hidden to visible
  // (BrowserHost dispatches `cluihud:browser-became-visible`) — important
  // because the panel mount-once architecture means this useEffect doesn't
  // re-run across mode/tab switches; we'd otherwise lose arrow nav after
  // the first render.
  useEffect(() => {
    function focus() {
      containerRef.current?.focus({ preventScroll: true });
    }
    focus();
    document.addEventListener("cluihud:browser-became-visible", focus);
    return () =>
      document.removeEventListener("cluihud:browser-became-visible", focus);
  }, []);

  async function open(port: number) {
    const url = `http://localhost:${port}`;
    const activeTab = session.tabs.find((t) => t.id === session.activeTabId);
    try {
      // BlankHomepage shows when the active tab is about:blank OR when
      // there are no tabs at all. In the about:blank case we navigate the
      // current tab in-place so we don't pile up empty placeholder tabs;
      // only when there's no tab to navigate do we create a new one.
      if (activeTab && activeTab.url === "about:blank") {
        await navigate({ sessionId, url, tabId: activeTab.id });
      } else {
        await newTab({ sessionId, url });
      }
    } catch {
      /* validate_url won't fail on a known-good localhost URL */
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (ports.length === 0) return;
    const cols = 2;
    let next = selected;
    switch (e.key) {
      case "ArrowRight":
        next = Math.min(selected + 1, ports.length - 1);
        break;
      case "ArrowLeft":
        next = Math.max(selected - 1, 0);
        break;
      case "ArrowDown":
        next = Math.min(selected + cols, ports.length - 1);
        break;
      case "ArrowUp":
        next = Math.max(selected - cols, 0);
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = ports.length - 1;
        break;
      case "Enter":
      case " ": {
        e.preventDefault();
        const port = ports[selected];
        if (port != null) void open(port);
        return;
      }
      default:
        return;
    }
    e.preventDefault();
    setSelected(next);
  }

  return (
    <div
      ref={containerRef}
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      className="absolute inset-0 flex flex-col items-center justify-center gap-6 bg-background p-8 text-center outline-none"
    >
      <div className="flex flex-col items-center gap-2 text-muted-foreground">
        <Globe className="size-8 opacity-40" />
        <p className="text-xs">Live preview</p>
      </div>
      {ports.length === 0 ? (
        <div className="max-w-xs space-y-2 text-xs text-muted-foreground/80">
          <p>No localhost dev servers detected.</p>
          <p className="text-muted-foreground/60">
            Start one (e.g. <code className="font-mono text-[11px]">pnpm dev</code> on{" "}
            <code className="font-mono text-[11px]">:5173</code>) or type a URL above.
          </p>
        </div>
      ) : (
        <div className="flex w-full max-w-md flex-col gap-2">
          <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-muted-foreground/70">
            <span>Detected dev servers</span>
            <span className="font-mono normal-case tracking-normal text-muted-foreground/50">
              ↑↓←→ + Enter
            </span>
          </div>
          <div className="grid grid-cols-2 gap-2">
            {ports.map((port, idx) => {
              const isSelected = idx === selected;
              return (
                <button
                  key={port}
                  type="button"
                  onClick={() => {
                    setSelected(idx);
                    void open(port);
                  }}
                  onMouseEnter={() => setSelected(idx)}
                  className={`flex flex-col items-start gap-0.5 rounded-md border px-3 py-2 text-left transition-colors ${
                    isSelected
                      ? "border-primary/60 bg-secondary"
                      : "border-border/60 bg-card hover:border-primary/40 hover:bg-secondary"
                  }`}
                >
                  <span className="font-mono text-sm text-foreground">:{port}</span>
                  <span className="font-mono text-[10px] text-muted-foreground">
                    http://localhost:{port}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function Placeholder({ text }: { text: string }) {
  return (
    <div className="grid h-full place-items-center text-xs text-muted-foreground">
      {text}
    </div>
  );
}
