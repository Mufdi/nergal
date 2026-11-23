export interface WaitDeps {
  now: () => number;
  nextFrame: () => Promise<void>;
}

export async function waitUntil(
  pred: () => boolean,
  timeoutMs: number,
  deps: WaitDeps,
): Promise<void> {
  const start = deps.now();
  while (!pred() && deps.now() - start < timeoutMs) {
    await deps.nextFrame();
  }
}

export function refitNeeded(used: boolean, current: boolean): boolean {
  return used !== current;
}
