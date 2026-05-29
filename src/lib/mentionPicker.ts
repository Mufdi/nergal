export interface MentionToken {
  query: string;
  start: number;
  end: number;
}

// Detect an active `@@<query>` run ending exactly at `caret`. Returns null
// when the caret isn't inside such a run — a whitespace or `@` between the
// `@@` and the caret breaks the token (so `@@ foo` or finished citations
// don't keep the picker open).
export function findActiveMention(text: string, caret: number): MentionToken | null {
  const head = text.slice(0, caret);
  const match = /@@([^\s@]*)$/.exec(head);
  if (!match) return null;
  return { query: match[1], start: match.index, end: caret };
}

export function buildCitation(noteName: string): string {
  return `> Source: [[${noteName}]]\n`;
}

export function applyMention(
  text: string,
  token: MentionToken,
  insert: string,
): { text: string; caret: number } {
  const before = text.slice(0, token.start);
  const after = text.slice(token.end);
  return { text: before + insert + after, caret: before.length + insert.length };
}
