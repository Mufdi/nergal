import { EditorView } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { appStore } from "@/stores/jotaiStore";
import { obsidianConfigAtom } from "@/stores/obsidian";
import { openObsidianHref } from "@/lib/markdown/obsidianMarkdown";
import { WIKILINK_PATTERN, buildObsidianUri } from "@/lib/markdown/remarkObsidianLinks";

/// Resolved the same way the remark plugin does (kept in sync). Read live, not
/// captured, so the extension survives a vault reconfig without a remount.
function resolveVaultName(): string | null {
  const cfg = appStore.get(obsidianConfigAtom);
  if (!cfg?.vault_root) return null;
  return cfg.vault_name?.trim() || cfg.vault_root.split("/").filter(Boolean).pop() || null;
}

/// Modifier-gated so plain clicks still position the caret — editing a line
/// that contains a wikilink stays intact (the code-editor convention).
export function obsidianWikilinkClickExtension(): Extension {
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      if (!(event.metaKey || event.ctrlKey) || event.button !== 0) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos == null) return false;

      const line = view.state.doc.lineAt(pos);
      const offsetInLine = pos - line.from;
      const re = new RegExp(WIKILINK_PATTERN, "g");
      let match: RegExpExecArray | null;
      while ((match = re.exec(line.text)) !== null) {
        const start = match.index;
        const end = start + match[0].length;
        if (offsetInLine < start || offsetInLine > end) continue;
        const vault = resolveVaultName();
        if (!vault) return false;
        const [, note, heading, block] = match;
        openObsidianHref(buildObsidianUri(vault, note, heading, block));
        event.preventDefault();
        return true;
      }
      return false;
    },
  });
}
