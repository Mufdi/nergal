export function posixQuote(s: string): string {
  return `'${s.replaceAll("'", "'\\''")}'`;
}
