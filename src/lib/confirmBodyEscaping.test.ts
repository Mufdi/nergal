import { describe, it, expect } from "vitest";

/// Enforces the html-escaping-contract invariant: no `confirm()`/`swalConfirm()`
/// call may interpolate a value into its `body` template literal without
/// routing it through the shared `escapeHtml`. Uses `import.meta.glob` (Vite's
/// raw-import, not node:fs — this project has no @types/node) to read every
/// source file at test time, so a brand-new call site is covered
/// automatically, not just the sites known when this test was written.

/// Expressions that are provably safe to interpolate unescaped because they
/// can only ever produce digits, empty string, or a pre-built HTML fragment
/// assembled from other already-escaped/safe pieces (never a raw caller name).
/// Any new entry here must carry that same guarantee — do not add a name-like
/// expression to dodge a real finding.
const SAFE_INTERPOLATION_ALLOWLIST = new Set<string>([
  "sessionsLine", // pre-built HTML fragment (Sidebar.tsx) built from a numeric count only
  "info.pid", // number | null, never attacker-shaped text
  "annotations.length", // number
  'annotations.length === 1 ? "" : "s"', // pluralization ternary, only ever "" or "s"
  "name(current)", // clickup.ts requestBindTaskAction: local `name` helper already wraps escapeHtml
  "name(taskId)", // same helper, same file
]);

/// Extracts the raw source text of one property's value out of an object
/// literal, honoring paren/brace/bracket depth and skipping characters inside
/// backtick template literals — so `body: cond ? \`a${x}\` : \`b${y}\`,` is
/// captured whole, and a `)`/`}` inside a nested call/template doesn't end
/// the value early.
function extractPropertyValue(objSrc: string, propName: string): string | null {
  const propMatch = new RegExp(`\\b${propName}\\s*:\\s*`).exec(objSrc);
  if (!propMatch) return null;
  const start = propMatch.index + propMatch[0].length;
  let depth = 0;
  let inTemplate = false;
  let i = start;
  for (; i < objSrc.length; i++) {
    const ch = objSrc[i];
    if (ch === "`") {
      inTemplate = !inTemplate;
      continue;
    }
    if (inTemplate) continue;
    if (ch === "(" || ch === "{" || ch === "[") depth++;
    else if (ch === ")" || ch === "}" || ch === "]") {
      if (depth === 0) break; // hit the closing brace of the enclosing object literal
      depth--;
    } else if (ch === "," && depth === 0) break;
  }
  return objSrc.slice(start, i);
}

/// True iff `expr` is exactly one top-level `escapeHtml(...)` call (the opening
/// paren closes at the end of the expression), so a compound like
/// `escapeHtml(a) + rawVar` is NOT accepted as "escaped".
function isSingleEscapeHtmlCall(expr: string): boolean {
  const prefix = "escapeHtml(";
  if (!expr.startsWith(prefix)) return false;
  let depth = 1;
  for (let i = prefix.length; i < expr.length; i++) {
    if (expr[i] === "(") depth++;
    else if (expr[i] === ")") {
      depth--;
      if (depth === 0) return i === expr.length - 1;
    }
  }
  return false;
}

interface Violation {
  file: string;
  expr: string;
}

function findUnescapedConfirmBodies(source: string, filePath: string): Violation[] {
  const violations: Violation[] = [];
  const callRegex = /\b(?:swalConfirm|confirm)\(\s*\{/g;
  let callMatch: RegExpExecArray | null;
  while ((callMatch = callRegex.exec(source))) {
    // Balance braces from the opening "{" of the call's object literal to find its extent.
    let depth = 1;
    let i = callMatch.index + callMatch[0].length;
    const objStart = i;
    while (i < source.length && depth > 0) {
      if (source[i] === "{") depth++;
      else if (source[i] === "}") depth--;
      i++;
    }
    const objSrc = source.slice(objStart, i - 1);
    const bodyValue = extractPropertyValue(objSrc, "body");
    if (!bodyValue) continue;

    const templateRegex = /`([^`]*)`/g;
    let templateMatch: RegExpExecArray | null;
    while ((templateMatch = templateRegex.exec(bodyValue))) {
      const template = templateMatch[1];
      const exprRegex = /\$\{([^}]*)\}/g;
      let exprMatch: RegExpExecArray | null;
      while ((exprMatch = exprRegex.exec(template))) {
        const expr = exprMatch[1].trim();
        // The WHOLE expression must be a single top-level escapeHtml(...) call —
        // not merely start with one, so `escapeHtml(a) + rawVar` (a partial
        // escape that "looks safe") is still flagged.
        if (isSingleEscapeHtmlCall(expr)) continue;
        if (SAFE_INTERPOLATION_ALLOWLIST.has(expr)) continue;
        violations.push({ file: filePath, expr });
      }
    }
  }
  return violations;
}

describe("confirm body escaping invariant (html-escaping-contract)", () => {
  it("has no unescaped interpolation in any confirm()/swalConfirm() body across src/", () => {
    // Exclude *.test.ts(x): their fixture strings intentionally contain
    // unescaped confirm() calls and would otherwise self-flag.
    const modules = import.meta.glob("/src/**/*.{ts,tsx}", {
      eager: true,
      query: "?raw",
      import: "default",
    }) as Record<string, string>;

    const violations = Object.entries(modules)
      .filter(([path]) => !path.endsWith(".test.ts") && !path.endsWith(".test.tsx"))
      .flatMap(([path, source]) => findUnescapedConfirmBodies(source, path));

    expect(violations).toEqual([]);
  });

  it("fails the scan when a 5th unescaped site is introduced (regression for the scan itself)", () => {
    const fixture = `
      async function removeThing(thing: { name: string }) {
        const ok = await swalConfirm({
          title: "Remove?",
          body: \`<strong>\${thing.name}</strong> will be removed.\`,
          destructive: true,
        });
        return ok;
      }
    `;
    const violations = findUnescapedConfirmBodies(fixture, "fixture.ts");
    expect(violations).toEqual([{ file: "fixture.ts", expr: "thing.name" }]);
  });

  it("does not flag a body that escapes its interpolation", () => {
    const fixture = `
      swalConfirm({
        title: "Remove?",
        body: \`<strong>\${escapeHtml(thing.name)}</strong> will be removed.\`,
      });
    `;
    expect(findUnescapedConfirmBodies(fixture, "fixture.ts")).toEqual([]);
  });

  it("flags a partial escape that concatenates a raw value", () => {
    // `escapeHtml(a) + rawVar` starts with escapeHtml( but the raw tail is
    // unescaped — must NOT be accepted just because it looks escaped.
    const fixture = `
      swalConfirm({
        body: \`<strong>\${escapeHtml(a) + rawVar}</strong> gone.\`,
      });
    `;
    expect(findUnescapedConfirmBodies(fixture, "fixture.ts")).toEqual([
      { file: "fixture.ts", expr: "escapeHtml(a) + rawVar" },
    ]);
  });

  // KNOWN SCAN BLIND SPOTS (defense-in-depth lint, not a hard guarantee — a
  // determined future edit can still evade it; documented so a reviewer knows
  // the limits): (1) a `body` that is a plain variable or built by string
  // concatenation instead of a template literal is not scanned; (2) a third
  // import alias of `confirm` beyond `confirm`/`swalConfirm` isn't matched by
  // the lexical call regex. No current call site does either. Anchoring these
  // would require a real AST pass — a follow-up if a site ever needs it.
});
