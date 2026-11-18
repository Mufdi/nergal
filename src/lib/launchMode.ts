export function launchAndConsume(
  id: string,
  launchModes: Record<string, "new" | "continue">,
  fresh: ReadonlySet<string>,
): { mode: "new" | "continue"; nextFresh: Set<string> } {
  const mode = launchModes[id] ?? (fresh.has(id) ? "new" : "continue");
  const nextFresh = new Set(fresh);
  if (mode === "new") nextFresh.delete(id);
  return { mode, nextFresh };
}
