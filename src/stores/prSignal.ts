export interface GithubPr {
  number: number | null;
  url: string | null;
  review_state: string | null;
  kind: string | null;
}

export interface PrSignalEntry {
  fetchedAt: number;
  signal?: string;
}

export const PR_FALLBACK_MS = 60_000;

export function prSignature(pr: GithubPr | null | undefined): string {
  if (!pr || pr.number == null || pr.kind) return "none";
  return `pr:${pr.number}`;
}

export function shouldRefreshPr(
  entry: PrSignalEntry | undefined,
  signal: string | undefined,
  now: number,
): boolean {
  if (!entry) return true;
  if (signal !== undefined && signal !== entry.signal) return true;
  return now - entry.fetchedAt > PR_FALLBACK_MS;
}
