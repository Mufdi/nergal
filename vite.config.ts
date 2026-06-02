import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  optimizeDeps: {
    entries: ["src/main.tsx"],
    include: [
      "react",
      "react-dom",
      "react/jsx-runtime",
      "react-markdown",
      "remark-gfm",
      "jotai",
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
    ],
  },
  build: {
    // Single bundle on purpose. Manual vendor chunking split React into a chunk
    // that evaluated after a module needing it (createContext of undefined →
    // React never mounts → "loading" forever) — a load-order bug invisible in
    // `tauri dev`, which doesn't bundle. The bundle loads from disk in a desktop
    // app, so its size is a non-issue; raise the warning past it rather than
    // re-introducing fragile manual chunks. For real startup wins, lazy-load
    // heavy views (codemirror) with dynamic import() — safe, unlike manualChunks.
    chunkSizeWarningLimit: 2600,
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/vendor/**", "**/target/**", "**/.worktrees/**"] },
  },
});
