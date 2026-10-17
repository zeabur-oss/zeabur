import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import Icons from "unplugin-icons/vite"
import { defineConfig } from "vite"

// The Rust server the dev server forwards `/api` to (`cargo run -p engawa-api --example demo`).
const api = process.env.ENGAWA_API ?? "http://127.0.0.1:8080"

export default defineConfig({
  // React Compiler through oxc-transform-react (experimental); the fallback is
  // @rolldown/plugin-babel with reactCompilerPreset().
  plugins: [
    react({ compiler: true }),
    tailwindcss(),
    // `~icons/ph/<name>`: one Phosphor icon, 1em square (scale 1) like Phosphor's own components.
    Icons({ scale: 1, compiler: "jsx", jsx: "react" }),
  ],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  build: {
    // Every supported browser has <link rel="modulepreload">; the polyfill would only add code
    // and a fetch() to the entry script.
    modulePreload: { polyfill: false },
    // Hashed file names live under assets/; the server caches exactly these as immutable.
    assetsDir: "assets",
    // Fonts and images stay files (served with their own cache headers), never data: URLs.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  server: {
    proxy: {
      "/api": { target: api, changeOrigin: false, ws: true },
    },
  },
})
