import { useAtomValue } from "jotai";
import { defaultUrlTransform } from "react-markdown";
import { invoke } from "@/lib/tauri";
import { obsidianConfigAtom } from "@/stores/obsidian";
import { remarkObsidianLinks, type RemarkObsidianOptions } from "./remarkObsidianLinks";

// react-markdown's default URL filter strips schemes outside http/https/
// mailto/tel/etc. as an XSS guard, so obsidian:// links arrive at the `a`
// component with href="" and a click triggers a same-page nav (reloads the
// webview). Override the transform to whitelist our two custom schemes
// while keeping the default safety for everything else.
export function obsidianUrlTransform(uri: string): string {
  if (uri.startsWith("obsidian://") || uri.startsWith("nergal://")) {
    return uri;
  }
  return defaultUrlTransform(uri);
}

export function useObsidianRemarkOptions(): RemarkObsidianOptions {
  const cfg = useAtomValue(obsidianConfigAtom);
  if (!cfg?.vault_root || !cfg.render_wikilinks) {
    return { enabled: false };
  }
  const vaultName =
    cfg.vault_name?.trim() ||
    cfg.vault_root.split("/").filter(Boolean).pop() ||
    null;
  return { enabled: true, vaultName };
}

export function useObsidianRemarkPlugin(): [typeof remarkObsidianLinks, RemarkObsidianOptions] {
  const opts = useObsidianRemarkOptions();
  return [remarkObsidianLinks, opts];
}

export function isObsidianHref(href: string | undefined | null): boolean {
  return typeof href === "string" && href.startsWith("obsidian://");
}

// Routed through the backend instead of tauri-plugin-shell's open() because
// the latter has a hardcoded scheme regex that rejects obsidian:// by
// default. The backend uses xdg-open + validates the scheme prefix itself.
export function openObsidianHref(href: string): void {
  invoke("obsidian_open_uri", { uri: href }).catch((err) =>
    console.warn("[obsidian] open_uri failed:", err),
  );
}
