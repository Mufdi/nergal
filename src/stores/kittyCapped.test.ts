import { describe, expect, it } from "vitest";
import { KITTY_CAPPED_MESSAGE, kittyCappedNotice } from "./kittyCapped";

describe("kittyCappedNotice", () => {
  it("notifies the first cap event of a terminal", () => {
    const { notify, next } = kittyCappedNotice(new Set<string>(), { session_id: "s1", pty_id: "p1" });

    expect(notify).toBe(true);
    expect(next.size).toBe(1);
  });

  it("does not notify again for a second event of the same terminal", () => {
    const first = kittyCappedNotice(new Set<string>(), { session_id: "s1", pty_id: "p1" });
    const second = kittyCappedNotice(first.next, { session_id: "s1", pty_id: "p1" });

    expect(second.notify).toBe(false);
    expect(second.next).toEqual(first.next);
  });

  it("notifies for another terminal of the same session", () => {
    const first = kittyCappedNotice(new Set<string>(), { session_id: "s1", pty_id: "p1" });
    const other = kittyCappedNotice(first.next, { session_id: "s1", pty_id: "p2" });

    expect(other.notify).toBe(true);
    expect(other.next.size).toBe(2);
  });

  it("does not mutate the previous set", () => {
    const prev = new Set<string>();
    kittyCappedNotice(prev, { session_id: "s1", pty_id: "p1" });

    expect(prev.size).toBe(0);
  });

  it("words the toast in neutral Spanish, never voseo", () => {
    expect(KITTY_CAPPED_MESSAGE).toBe(
      "Imágenes kitty pausadas en esta terminal. Ábrela de nuevo para volver a verlas.",
    );
  });
});
