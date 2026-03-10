import { useAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { invoke } from "@/lib/tauri";
import type { Config } from "@/lib/types";

interface SettingsPanelProps {
  onClose: () => void;
}

const FIELDS: { key: keyof Config; label: string; placeholder: string }[] = [
  { key: "claude_binary", label: "Claude Binary", placeholder: "/usr/bin/claude" },
  { key: "plans_directory", label: "Plans Directory", placeholder: "~/.claude/plans" },
  { key: "transcripts_directory", label: "Transcripts Directory", placeholder: "~/.claude/transcripts" },
  { key: "default_shell", label: "Default Shell", placeholder: "/bin/bash" },
  { key: "theme_mode", label: "Theme", placeholder: "dark" },
];

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [config, setConfig] = useAtom(configAtom);

  function handleChange(key: keyof Config, value: string) {
    setConfig((prev) => ({ ...prev, [key]: value }));
  }

  function handleSave() {
    invoke("save_config", { config }).catch(() => {});
    onClose();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
    >
      <div className="w-full max-w-md border border-border bg-surface-raised">
        <header className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-sm font-medium text-text">Settings</h2>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text"
            aria-label="Close settings"
          >
            x
          </button>
        </header>

        <div className="space-y-3 px-4 py-4">
          {FIELDS.map(({ key, label, placeholder }) => (
            <div key={key}>
              <label htmlFor={`setting-${key}`} className="mb-1 block text-xs text-text-muted">
                {label}
              </label>
              <input
                id={`setting-${key}`}
                type="text"
                value={config[key]}
                onChange={(e) => handleChange(key, e.target.value)}
                placeholder={placeholder}
                className="w-full border border-border bg-surface px-2 py-1.5 text-xs text-text outline-none focus:border-accent"
              />
            </div>
          ))}
        </div>

        <footer className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <button
            onClick={onClose}
            className="px-3 py-1 text-xs text-text-muted hover:text-text"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="bg-accent/20 px-3 py-1 text-xs text-accent hover:bg-accent/30"
          >
            Save
          </button>
        </footer>
      </div>
    </div>
  );
}
