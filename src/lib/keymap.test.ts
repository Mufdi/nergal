import { describe, it, expect } from "vitest";
import {
  parseKeys,
  isChord,
  chordContinuation,
  parseChord,
  comboSignature,
  validateCombo,
  formatKeyParts,
} from "@/lib/keymap";

describe("parseKeys", () => {
  it("parses plain combos", () => {
    expect(parseKeys("ctrl+shift+b")).toEqual({ ctrl: true, shift: true, alt: false, code: "KeyB" });
    expect(parseKeys("alt+p")).toEqual({ ctrl: false, shift: false, alt: true, code: "KeyP" });
  });

  it("returns null for chord strings", () => {
    expect(parseKeys("leader n")).toBeNull();
    expect(parseKeys("leader shift+w")).toBeNull();
  });

  it("returns null for the palette-only sentinel", () => {
    expect(parseKeys("")).toBeNull();
  });

  it("parses the space token to code Space", () => {
    expect(parseKeys("ctrl+space")).toEqual({ ctrl: true, shift: false, alt: false, code: "Space" });
  });
});

describe("chord helpers", () => {
  it("isChord detects the leader prefix", () => {
    expect(isChord("leader n")).toBe(true);
    expect(isChord("leader .")).toBe(true);
    expect(isChord("leader 0")).toBe(true);
    expect(isChord("ctrl+shift+o")).toBe(false);
    expect(isChord("")).toBe(false);
  });

  it("chordContinuation extracts the combo after the prefix", () => {
    expect(chordContinuation("leader n")).toBe("n");
    expect(chordContinuation("leader shift+w")).toBe("shift+w");
    expect(chordContinuation("ctrl+shift+o")).toBeNull();
  });

  it("parseChord parses the continuation as a plain combo", () => {
    expect(parseChord("leader n")).toEqual({ ctrl: false, shift: false, alt: false, code: "KeyN" });
    expect(parseChord("leader shift+w")).toEqual({ ctrl: false, shift: true, alt: false, code: "KeyW" });
    expect(parseChord("ctrl+shift+o")).toBeNull();
  });
});

describe("comboSignature", () => {
  it("keeps plain combo format unchanged", () => {
    expect(comboSignature("ctrl+shift+b")).toBe("cs:KeyB");
  });

  it("namespaces chord signatures under L:", () => {
    expect(comboSignature("leader o")).toBe("L::KeyO");
  });

  it("does not collide global and chord signatures sharing a key", () => {
    expect(comboSignature("ctrl+shift+o")).not.toBe(comboSignature("leader o"));
  });

  it("collides two identical chords", () => {
    expect(comboSignature("leader w")).toBe(comboSignature("leader w"));
  });

  it("does not collide leader shift+w with leader w", () => {
    expect(comboSignature("leader shift+w")).not.toBe(comboSignature("leader w"));
  });

  it("returns null for unparseable or palette-only combos", () => {
    expect(comboSignature("")).toBeNull();
    expect(comboSignature("ctrl+}")).toBeNull();
  });
});

describe("validateCombo", () => {
  const effective = [
    { id: "ship-session", keys: "ctrl+shift+enter" },
    { id: "obsidian-panel", keys: "ctrl+shift+o" },
    { id: "new-session", keys: "leader n" },
  ];

  it("rejects a bare-letter global combo", () => {
    const result = validateCombo("g", "some-id", effective);
    expect(result.ok).toBe(false);
  });

  it("accepts a bare-letter leader continuation", () => {
    const result = validateCombo("leader g", "some-id", effective);
    expect(result.ok).toBe(true);
  });

  it("accepts a shift+key leader continuation", () => {
    const result = validateCombo("leader shift+w", "some-id", effective);
    expect(result.ok).toBe(true);
  });

  it("rejects ctrl in a leader continuation", () => {
    const result = validateCombo("leader ctrl+p", "some-id", effective);
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/Ctrl\/Alt/);
  });

  it("rejects bare . as a leader continuation (raw-mode sentinel)", () => {
    const result = validateCombo("leader .", "some-id", effective);
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/raw mode/);
  });

  it("rejects the reserved combo globally", () => {
    const result = validateCombo("ctrl+shift+u", "some-id", effective);
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/Reserved/);
    expect(result.reason).toMatch(/IBus/);
  });

  it("rejects declared DE bans with their reason", () => {
    const result = validateCombo("ctrl+alt+t", "some-id", effective);
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/GNOME open-terminal/);
  });

  it("does not apply the reserved check inside the leader namespace", () => {
    const result = validateCombo("leader u", "some-id", effective);
    expect(result.ok).toBe(true);
  });

  it("flags a collision within the leader namespace", () => {
    const result = validateCombo("leader n", "other-id", effective);
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/new-session/);
  });

  it("does not flag a leader continuation colliding with a same-key global combo", () => {
    const result = validateCombo("leader o", "some-id", effective);
    expect(result.ok).toBe(true);
  });
});

describe("formatKeyParts", () => {
  it("renders the space token as Space", () => {
    expect(formatKeyParts("ctrl+space")).toEqual(["Ctrl", "Space"]);
  });
});
