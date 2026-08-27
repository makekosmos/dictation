import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const makekosmosRoot = path.resolve(__dirname, "..");

export default defineConfig({
  base: "./",
  plugins: [vue({ features: { vaporInterop: true } }), tailwindcss()],
  resolve: {
    alias: {
      "@kosmos/visuals/theme/css": path.resolve(makekosmosRoot, "imago/theme/css-variables.css"),
      "@kosmos/visuals": path.resolve(makekosmosRoot, "imago/index.ts"),
    },
    dedupe: ["vue"],
  },
  build: { outDir: "dist", emptyOutDir: true, assetsDir: "assets" },
  server: { fs: { allow: [makekosmosRoot] }, hmr: { overlay: false } },
  clearScreen: false,
});
