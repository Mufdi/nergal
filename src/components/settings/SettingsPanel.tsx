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

const FIELDS: { key: keyof Config; label: string; placeholder: string }[] = [
  { key: "claude_binary", label: "Claude Binary", placeholder: "/usr/bin/claude" },
  { key: "plans_directory", label: "Plans Directory", placeholder: "~/.claude/plans" },
  { key: "transcripts_directory", label: "Transcripts Directory", placeholder: "~/.claude/transcripts" },
  { key: "default_shell", label: "Default Shell", placeholder: "/bin/bash" },
  { key: "theme_mode", label: "Theme", placeholder: "dark" },
];

export function SettingsPanel({ open, onOpenChange }: SettingsProps) {
  const [config, setConfig] = useAtom(configAtom);

  function handleChange(key: keyof Config, value: string) {
    setConfig((prev) => ({ ...prev, [key]: value }));
  }

  function handleSave() {
    invoke("save_config", { config }).catch(() => {});
    onOpenChange(false);
  }

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
          {FIELDS.map(({ key, label, placeholder }) => (
            <div key={key} className="grid gap-2">
              <Label htmlFor={`setting-${key}`}>{label}</Label>
              <Input
                id={`setting-${key}`}
                type="text"
                value={config[key]}
                onChange={(e) => handleChange(key, e.target.value)}
                placeholder={placeholder}
              />
            </div>
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
