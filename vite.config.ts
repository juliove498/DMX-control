import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  build: {
    // Vite 7's default target is Safari 16+. The 2015 Intel MacBook's
    // WKWebView is whatever Safari its last macOS shipped, so transpile
    // down far enough that the bundle at least *parses* there. Safari 15
    // is the floor esbuild can actually hit (it refuses to lower
    // destructuring for safari14 and below), and every macOS we support
    // (10.15+) can update Safari to at least 15.6. Runtime APIs aren't
    // polyfilled by esbuild — see src/lib/compat.ts.
    target: ["es2020", "safari15"],
  },

  resolve: {
    alias: {
      "@bindings": path.resolve(__dirname, "src-tauri/bindings"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. ignore the rust crate; ts-rs bindings DO need to trigger reloads
      ignored: ["**/src-tauri/target/**", "**/src-tauri/gen/**"],
    },
  },
}));
