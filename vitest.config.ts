import path from "path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    // scripts/*.test.mjs run on node:test via `pnpm release:test` — keep vitest scoped to src/.
    include: ["src/**/*.test.ts"],
  },
});
