import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  build: {
    rollupOptions: {
      output: {
        manualChunks: (id) =>
          id.includes("/node_modules/@xterm/") ? "terminal" : undefined,
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": {
        target: loadEnv(mode, ".", "LINUXCATCH_").LINUXCATCH_API_URL || "http://127.0.0.1:3000",
        ws: true,
      },
    },
  },
}));
