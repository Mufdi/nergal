const WIKILINK_RE =
  /(?<!\\)\[\[([^\[\]|#^]+?)(?:#([^\[\]|^]+))?(?:\^([^\[\]|]+))?(?:\|([^\[\]]+))?\]\]/g;

export interface RemarkObsidianOptions {
  vaultName?: string | null;
  enabled?: boolean;
}

interface AnyNode {
  type: string;
  children?: AnyNode[];
  value?: string;
  url?: string;
}

function encode(s: string): string {
  return encodeURIComponent(s);
}

function buildUri(vault: string, note: string, heading?: string, block?: string): string {
  let uri = `obsidian://open?vault=${encode(vault)}&file=${encode(note)}`;
  if (heading) uri += `#${encode(heading)}`;
  else if (block) uri += `#^${encode(block)}`;
  return uri;
}

function splitTextNode(value: string, vault: string): AnyNode[] | null {
  WIKILINK_RE.lastIndex = 0;
  const parts: AnyNode[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = WIKILINK_RE.exec(value)) !== null) {
    const [full, note, heading, block, alias] = match;
    if (match.index > lastIndex) {
      parts.push({ type: "text", value: value.slice(lastIndex, match.index) });
    }
    parts.push({
      type: "link",
      url: buildUri(vault, note, heading, block),
      children: [{ type: "text", value: alias ?? note }],
    });
    lastIndex = match.index + full.length;
  }
  if (parts.length === 0) return null;
  if (lastIndex < value.length) {
    parts.push({ type: "text", value: value.slice(lastIndex) });
  }
  return parts;
}

export function remarkObsidianLinks(opts: RemarkObsidianOptions = {}) {
  const { vaultName, enabled = true } = opts;
  return (tree: AnyNode) => {
    if (!enabled || !vaultName) return;
    walk(tree, vaultName);
  };
}

function walk(node: AnyNode, vault: string): void {
  if (!node.children) return;
  const next: AnyNode[] = [];
  for (const child of node.children) {
    // code blocks and inline code preserve their literal contents — wikilinks
    // inside `[[code]]` examples must not become real links.
    if (child.type === "code" || child.type === "inlineCode") {
      next.push(child);
      continue;
    }
    if (child.type === "text" && typeof child.value === "string") {
      const replaced = splitTextNode(child.value, vault);
      if (replaced) {
        next.push(...replaced);
        continue;
      }
    }
    walk(child, vault);
    next.push(child);
  }
  node.children = next;
}
