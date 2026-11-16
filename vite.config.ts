import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import pkg from "./package.json" with { type: "json" };

// Tauri expects a fixed port and ignores src-tauri so Rust rebuilds don't trigger HMR.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  build: {
    // WKWebView (macOS 13+), WebView2 and WebKitGTK all handle modern syntax.
    target: ["es2022", "safari16", "chrome120"],
    cssTarget: ["safari16", "chrome120"],
    // The UI loads from local disk as one chunk; warn only if it grows past ~400 KB minified.
    chunkSizeWarningLimit: 400,
  },
});
