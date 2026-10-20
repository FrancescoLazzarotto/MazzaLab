import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// During development the UI runs on Vite's server and talks to a
// mazzalab-server on its default HTTP port.
const server = process.env.MAZZALAB_SERVER ?? "http://127.0.0.1:53080";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": server,
      "/ws": { target: server.replace(/^http/, "ws"), ws: true },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
