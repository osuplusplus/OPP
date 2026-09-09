import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import packageJson from "./package.json";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(packageJson.version),
  },
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    globals: true,
    // jsdom 环境启动/并行 worker 在高负载机器上偶发挤兑,默认 5s 会
    // 随机击中慢启动的用例(每次是不同文件);用例单独运行均在 1s 内,
    // 放宽上限只为消除启动抖动。
    testTimeout: 20_000,
    // jsdom 29 起对 opaque origin(默认 about:blank)抛 SecurityError,
    // window.localStorage 直接不可用;给一个 http origin 恢复存储 API。
    environmentOptions: {
      jsdom: { url: "http://localhost/" },
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
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
