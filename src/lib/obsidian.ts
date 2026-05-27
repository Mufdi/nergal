import { invoke } from "@/lib/tauri";

export async function openInObsidian(
  workspaceId: string,
  absPath: string,
  opts?: { heading?: string; block?: string },
): Promise<void> {
  const uri = await invoke<string>("obsidian_build_uri", {
    workspaceId,
    path: absPath,
    heading: opts?.heading ?? null,
    block: opts?.block ?? null,
  });
  await invoke("obsidian_open_uri", { uri });
}
