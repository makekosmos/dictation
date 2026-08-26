import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  base: "./",
  plugins: [vue({ features: { vaporInterop: true } })],
  build: { outDir: "dist", emptyOutDir: true, assetsDir: "assets" },
  server: { hmr: { overlay: false } },
  clearScreen: false,
});
