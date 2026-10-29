import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

const DEV_SERVER_HOST = "127.0.0.1";
const DEV_SERVER_PORT = 1420;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    host: DEV_SERVER_HOST,
    port: DEV_SERVER_PORT,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  build: { target: "es2022" },
});
