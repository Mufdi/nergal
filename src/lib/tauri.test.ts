import { beforeEach, describe, expect, it, vi } from "vitest";

const listenMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { subscribe } from "./tauri";

describe("subscribe", () => {
  let resolveListen: (fn: () => void) => void;
  let emit: (payload: unknown) => void;
  const unlisten = vi.fn();

  beforeEach(() => {
    unlisten.mockReset();
    listenMock.mockReset();
    listenMock.mockImplementation((_event: string, cb: (e: { payload: unknown }) => void) => {
      emit = (payload) => cb({ payload });
      return new Promise<() => void>((resolve) => {
        resolveListen = resolve;
      });
    });
  });

  it("unlistens even when disposed before registration resolves", async () => {
    const dispose = subscribe("x", vi.fn());
    dispose();
    resolveListen(unlisten);
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("drops events delivered after dispose", () => {
    const handler = vi.fn();
    const dispose = subscribe("x", handler);
    emit(1);
    dispose();
    emit(2);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(1);
  });
});
