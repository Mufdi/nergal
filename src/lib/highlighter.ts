import Highlighter from "web-highlighter";
import type { DomMeta } from "web-highlighter/dist/types";
import type HighlightSource from "web-highlighter/dist/model/source";

export type { DomMeta, HighlightSource };
export { Highlighter };

export const HighlightEvent = Highlighter.event;

export function createHighlighter(container: HTMLElement): Highlighter {
  return new Highlighter({
    $root: container,
    wrapTag: "mark",
    exceptSelectors: ["[data-annotation-toolbar]", "button", "input", "textarea"],
    style: { className: "annotation-highlight" },
  });
}

/// Create a Range spanning all text nodes within an element.
/// Used for pinpoint mode: click element → create range → highlighter.fromRange().
export function createTextRange(element: HTMLElement): Range {
  const range = document.createRange();
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  const firstNode = walker.nextNode();
  if (!firstNode) {
    range.selectNodeContents(element);
    return range;
  }
  range.setStart(firstNode, 0);
  let lastNode: Node = firstNode;
  let node: Node | null;
  while ((node = walker.nextNode())) lastNode = node;
  range.setEnd(lastNode, lastNode.textContent?.length ?? 0);
  return range;
}

/// Find the nearest annotatable element from an event target.
/// Hold Ctrl to escalate to parent block (e.g., list item → entire list).
export function resolvePinpointTarget(target: EventTarget | null, event?: MouseEvent): HTMLElement | null {
  const el = target as HTMLElement | null;
  if (!el?.closest) return null;

  const item = el.closest("[data-annotatable]") as HTMLElement | null;
  if (!item) return null;

  // Ctrl held → escalate to parent annotatable (list-item → list, etc.)
  if (event?.ctrlKey) {
    const parent = item.parentElement?.closest("[data-annotatable]") as HTMLElement | null;
    if (parent) return parent;
  }

  return item;
}
