import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { TextInputDialog } from "@/components/ui/TextInputDialog";
import {
  renameBranchSignalAtom,
  gitHeaderMapAtom,
  gitInfoMapAtom,
  refreshGitInfoAtom,
  activeGitInfoAtom,
} from "@/stores/git";
import { toastsAtom } from "@/stores/toast";
import { activeSessionIdAtom, activeWorkspaceAtom } from "@/stores/workspace";

export function BranchRenameDialog() {
  const signal = useAtomValue(renameBranchSignalAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const gitHeaderMap = useAtomValue(gitHeaderMapAtom);
  const gitInfo = useAtomValue(activeGitInfoAtom);
  const setGitHeaderMap = useSetAtom(gitHeaderMapAtom);
  const setGitInfoMap = useSetAtom(gitInfoMapAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const addToast = useSetAtom(toastsAtom);
  const [open, setOpen] = useState(false);

  const branch = (sessionId ? gitHeaderMap[sessionId]?.branch : "") || gitInfo?.branch || "";

  useEffect(() => {
    if (signal === 0) return;
    if (!sessionId) {
      addToast({ message: "Rename branch", description: "No active session", type: "info" });
      return;
    }
    if (workspace && !workspace.is_git) {
      addToast({ message: "Rename branch", description: "Not a git workspace", type: "info" });
      return;
    }
    setOpen(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signal]);

  async function handleSubmit(name: string) {
    if (!sessionId || !name || name === branch) return;
    try {
      await invoke("git_rename_branch", { sessionId, newName: name });
      setGitHeaderMap((prev) => {
        const entry = prev[sessionId];
        return entry ? { ...prev, [sessionId]: { ...entry, branch: name } } : prev;
      });
      setGitInfoMap((prev) => {
        const entry = prev[sessionId];
        return entry ? { ...prev, [sessionId]: { ...entry, branch: name } } : prev;
      });
      addToast({ message: "Branch renamed", description: name, type: "success" });
      refreshGit(sessionId);
    } catch (err) {
      addToast({ message: "Rename failed", description: String(err), type: "error" });
    }
  }

  return (
    <TextInputDialog
      open={open}
      onOpenChange={setOpen}
      title="Rename branch"
      description="Local only — the remote branch and any open PR keep their name."
      initialValue={branch}
      placeholder="branch-name"
      confirmLabel="Rename"
      mono
      onSubmit={(v) => void handleSubmit(v)}
    />
  );
}
