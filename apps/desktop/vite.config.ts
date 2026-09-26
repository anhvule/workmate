import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri drives this dev server; the fixed port and strict mode mean a port
// clash fails loudly rather than silently serving the app somewhere the
// desktop shell is not looking.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", sourcemap: true },
});
