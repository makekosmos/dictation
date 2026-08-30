import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  base: "./",
  plugins: [vue({ features: { vaporInterop: true } }), tailwindcss()],
  resolve: { dedupe: ["vue"] },
  build: { outDir: "dist", emptyOutDir: true, assetsDir: "assets" },
  server: { hmr: { overlay: false } },
  clearScreen: false,
});
