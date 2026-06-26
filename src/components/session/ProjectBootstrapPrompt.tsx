import { useEffect, useState } from "react";
import { useAtom, useSetAtom } from "jotai";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@/lib/tauri";
import { bootstrapPromptAtom } from "@/stores/obsidian";
import { toastsAtom } from "@/stores/toast";
import { openInObsidian } from "@/lib/obsidian";

interface ProjectNoteResult {
  path: string;
  created: boolean;
}

export function ProjectBootstrapPrompt() {
  const [prompt, setPrompt] = useAtom(bootstrapPromptAtom);
  const setToasts = useSetAtom(toastsAtom);
  const [layout, setLayout] = useState(false);
  const [targetPath, setTargetPath] = useState("");
  const [busy, setBusy] = useState(false);

  const open = prompt != null;

  useEffect(() => {
    if (prompt) {
      setTargetPath(prompt.expectedPath);
      setLayout(false);
    }
  }, [prompt]);

  function handleClose() {
    if (busy) return;
    setPrompt(null);
  }

  async function handleConfirm() {
    if (!prompt) return;
    if (!targetPath.trim()) {
      setToasts({ message: "Path is empty", type: "error" });
      return;
    }
    setBusy(true);
    try {
      const result = await invoke<ProjectNoteResult>("obsidian_create_project_note", {
        workspaceId: prompt.workspaceId,
        targetPath: targetPath.trim(),
        suggestedLayout: layout,
      });
      setToasts({
        message: result.created ? "Vault note created" : "Vault note already exists — opening",
        description: result.path,
        type: "success",
      });
      try {
        await openInObsidian(prompt.workspaceId, result.path);
      } catch {
        /* user can open manually */
      }
      setPrompt(null);
    } catch (err) {
      setToasts({
        message: "Failed to create vault note",
        description: String(err),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  }

  if (!prompt) return null;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className="sm:max-w-xl overflow-hidden">
        <DialogHeader>
          <DialogTitle>Add a project note to your Obsidian vault?</DialogTitle>
          <DialogDescription className="break-words">
            Optional. Scaffolds a markdown file inside your vault for the workspace{" "}
            <span className="font-medium text-foreground">{prompt.workspaceName}</span> so
            you have a fixed anchor for decisions, session logs, and cross-links.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          <div className="grid gap-1.5">
            <Label htmlFor="bootstrap-target-path">Note path</Label>
            <Input
              id="bootstrap-target-path"
              type="text"
              value={targetPath}
              onChange={(e) => setTargetPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !busy && targetPath.trim()) {
                  e.preventDefault();
                  void handleConfirm();
                }
              }}
              spellCheck={false}
              autoComplete="off"
            />
            <p className="text-[11px] text-muted-foreground break-words">
              Edit if you prefer a different location. If the file already exists, it stays untouched and just opens.
            </p>
          </div>

          {prompt.inheritedVault && (
            <p className="text-[11px] text-muted-foreground break-words">
              Vault path inherited from your current workspace. Change it later in Settings → Obsidian.
            </p>
          )}

          <label className="flex items-start gap-3">
            <Switch checked={layout} onCheckedChange={setLayout} />
            <div className="grid gap-0.5 min-w-0">
              <span className="text-sm font-medium">Apply suggested layout</span>
              <span className="text-xs text-muted-foreground break-words">
                Also point this workspace's session-log and MOC channels at <code>Projects/&lt;name&gt;/</code>.
                Quick capture and templates stay as-is.
              </span>
            </div>
          </label>
        </div>

        <DialogFooter className="flex-nowrap gap-1.5">
          <Button variant="secondary" onClick={handleClose} disabled={busy}>
            Skip <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button onClick={handleConfirm} disabled={busy}>
            {busy ? "Creating…" : "Create"}
            <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
