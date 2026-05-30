import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const rootDir = path.resolve(__dirname, "../..");
const frontendDir = path.resolve(__dirname);

export default defineConfig({
  plugins: [react(), tailwindcss()],
  publicDir: path.resolve(frontendDir, "public"),
  resolve: {
    alias: {
      "@": path.resolve(frontendDir, "react")
    }
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true
  },
  build: {
    manifest: true,
    outDir: path.resolve(frontendDir, "dist/client"),
    emptyOutDir: true,
    rollupOptions: {
      input: path.resolve(rootDir, "src/frontend/entry-client.tsx"),
      output: {
        entryFileNames: "assets/[name]-[hash].js",
        chunkFileNames: "assets/[name]-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]"
      }
    }
  }
});
