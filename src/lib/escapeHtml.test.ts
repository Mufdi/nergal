import { describe, it, expect } from "vitest";
import { escapeHtml } from "@/lib/escapeHtml";

describe("escapeHtml", () => {
  it("escapes all five HTML metacharacters", () => {
    expect(escapeHtml("&")).toBe("&amp;");
    expect(escapeHtml("<")).toBe("&lt;");
    expect(escapeHtml(">")).toBe("&gt;");
    expect(escapeHtml('"')).toBe("&quot;");
    expect(escapeHtml("'")).toBe("&#39;");
  });

  it("leaves plain text unchanged", () => {
    expect(escapeHtml("my-worktree-2")).toBe("my-worktree-2");
    expect(escapeHtml("Session Alpha")).toBe("Session Alpha");
  });

  it("escapes & first so entities are not double-escaped", () => {
    expect(escapeHtml("&lt;")).toBe("&amp;lt;");
    expect(escapeHtml("&amp;")).toBe("&amp;amp;");
  });

  it("renders an XSS-shaped name as literal text", () => {
    const malicious = `<img src=x onerror=alert(1)>`;
    expect(escapeHtml(malicious)).toBe(
      "&lt;img src=x onerror=alert(1)&gt;",
    );
    expect(escapeHtml(malicious)).not.toContain("<img");
  });

  it("escapes a name containing a script tag and an ampersand", () => {
    const name = `Bob & <script>alert('xss')</script>`;
    const escaped = escapeHtml(name);
    expect(escaped).not.toContain("<script>");
    expect(escaped).toBe(
      "Bob &amp; &lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;",
    );
  });
});
