import { useState, useEffect } from "react";
import { useAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { invoke } from "@/lib/tauri";
import type { Config } from "@/lib/types";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

interface SettingsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

interface EditorInfo {
  id: string;
  name: string;
  command: string;
  available: boolean;
}

type StringConfigKey = {
  [K in keyof Config]: Config[K] extends string ? K : never;
}[keyof Config];

type BooleanConfigKey = {
  [K in keyof Config]: Config[K] extends boolean ? K : never;
}[keyof Config];

const TEXT_FIELDS: { key: StringConfigKey; label: string; placeholder: string }[] = [
  { key: "claude_binary", label: "Claude Binary", placeholder: "/usr/bin/claude" },
  { key: "plans_directory", label: "Plans Directory", placeholder: "~/.claude/plans" },
  { key: "transcripts_directory", label: "Transcripts Directory", placeholder: "~/.claude/transcripts" },
  { key: "default_shell", label: "Default Shell", placeholder: "/bin/bash" },
  { key: "theme_mode", label: "Theme", placeholder: "dark" },
];

const TOGGLE_FIELDS: { key: BooleanConfigKey; label: string; help: string }[] = [
  {
    key: "terminal_kitty_keyboard",
    label: "Kitty keyboard protocol",
    help: "Distinct encoding for Ctrl+Backspace, Shift+Enter, Alt+letter.",
  },
];

export function SettingsPanel({ open, onOpenChange }: SettingsProps) {
  const [config, setConfig] = useAtom(configAtom);
  const [editors, setEditors] = useState<EditorInfo[]>([]);

  useEffect(() => {
    if (open) {
      invoke<EditorInfo[]>("detect_editors").then(setEditors).catch(() => {});
    }
  }, [open]);

  function handleTextChange(key: StringConfigKey, value: string) {
    setConfig((prev: Config) => ({ ...prev, [key]: value }));
  }

  function handleToggleChange(key: BooleanConfigKey, value: boolean) {
    setConfig((prev: Config) => ({ ...prev, [key]: value }));
  }

  function handleSave() {
    invoke("save_config", { config }).catch(() => {});
    onOpenChange(false);
  }

  const available = editors.filter((e) => e.available);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure paths and preferences for cluihud.
          </DialogDescription>
        </DialogHeader>

        <Separator />

        <div className="space-y-4">
          {TEXT_FIELDS.map(({ key, label, placeholder }) => (
            <div key={key} className="grid gap-2">
              <Label htmlFor={`setting-${key}`}>{label}</Label>
              <Input
                id={`setting-${key}`}
                type="text"
                value={config[key]}
                onChange={(e) => handleTextChange(key, e.target.value)}
                placeholder={placeholder}
              />
            </div>
          ))}

          <div className="grid gap-2">
            <Label htmlFor="setting-preferred_editor">Preferred Editor</Label>
            <select
              id="setting-preferred_editor"
              value={config.preferred_editor}
              onChange={(e) => handleTextChange("preferred_editor", e.target.value)}
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <option value="">Auto-detect</option>
              {available.map((editor) => (
                <option key={editor.id} value={editor.id}>
                  {editor.name}
                </option>
              ))}
            </select>
          </div>

          <Separator />

          {TOGGLE_FIELDS.map(({ key, label, help }) => (
            <label key={key} className="flex items-start gap-3 text-sm">
              <input
                id={`setting-${key}`}
                type="checkbox"
                checked={config[key]}
                onChange={(e) => handleToggleChange(key, e.target.checked)}
                className="mt-0.5 h-4 w-4"
              />
              <span className="flex flex-col gap-0.5">
                <span className="font-medium">{label}</span>
                <span className="text-xs text-muted-foreground">{help}</span>
              </span>
            </label>
          ))}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
