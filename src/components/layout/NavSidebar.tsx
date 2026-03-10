import { useAtom, useAtomValue } from "jotai";
import { sessionsAtom, activeSessionIndexAtom } from "@/stores/session";
import { invoke } from "@/lib/tauri";

interface NavSidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function NavSidebar({ collapsed, onToggle }: NavSidebarProps) {
  const sessions = useAtomValue(sessionsAtom);
  const [activeIndex, setActiveIndex] = useAtom(activeSessionIndexAtom);

  function handleNewSession() {
    invoke("session_create", {}).catch(() => {});
  }

  return (
    <aside
      className={`flex flex-col border-r border-border bg-surface transition-[width] duration-150 ${
        collapsed ? "w-10" : "w-48"
      }`}
    >
      <div className="flex h-8 items-center justify-between border-b border-border px-2">
        {!collapsed && <span className="text-xs font-medium text-text-muted">Sessions</span>}
        <button
          onClick={onToggle}
          className="flex h-6 w-6 items-center justify-center text-text-muted hover:text-text"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? "\u25B6" : "\u25C0"}
        </button>
      </div>

      <nav className="flex-1 overflow-y-auto" aria-label="Session list">
        {sessions.map((session, index) => (
          <button
            key={session.id}
            onClick={() => setActiveIndex(index)}
            className={`flex w-full items-center gap-2 border-b border-border px-2 py-1.5 text-left text-xs ${
              index === activeIndex
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised"
            }`}
            aria-current={index === activeIndex ? "true" : undefined}
          >
            <span
              className={`h-1.5 w-1.5 flex-shrink-0 ${
                session.active ? "bg-success" : "bg-text-muted"
              }`}
              aria-hidden="true"
            />
            {!collapsed && (
              <span className="truncate">{session.cwd?.split("/").pop() ?? session.id.slice(0, 8)}</span>
            )}
          </button>
        ))}
      </nav>

      <div className="border-t border-border p-1">
        <button
          onClick={handleNewSession}
          className="flex h-6 w-full items-center justify-center text-text-muted hover:bg-surface-raised hover:text-text"
          aria-label="New session"
        >
          <span className="text-sm">+</span>
        </button>
      </div>
    </aside>
  );
}
