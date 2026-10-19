import { readdirSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import Icons from "unplugin-icons/vite"
import { defineConfig } from "vite"

// The Rust server the dev server forwards `/api` to (`cargo run -p engawa-api --example demo`).
const api = process.env.ENGAWA_API ?? "http://127.0.0.1:8080"

// @engawahq/design-system ships its components as sources: they are compiled like this app's own
// (React Compiler, `~icons/…`), unlike every other package in node_modules.
const designSystem = "/node_modules/@engawahq/design-system/"

// The packages the design system's sources import (`@base-ui/react/select`, `cn` …). Left out of
// dev pre-bundling, the design system's own imports are not pre-bundled either, and some reach
// CommonJS (Base UI's `use-sync-external-store/shim`) that the browser cannot load as it is, so they
// are pre-bundled by name. Read from the sources, so the list follows the pin.
const designSystemSources = resolve(import.meta.dirname, "node_modules/@engawahq/design-system/src")
const designSystemImports = [
  ...new Set(
    readdirSync(designSystemSources, { recursive: true, encoding: "utf8" })
      .filter((file) => /\.tsx?$/.test(file) && !file.endsWith(".test.ts"))
      .flatMap((file) =>
        [
          ...readFileSync(resolve(designSystemSources, file), "utf8").matchAll(
            /from "([^".~][^"]*)"/g,
          ),
        ].map((match) => match[1]!),
      ),
  ),
]

export default defineConfig({
  // React Compiler through oxc-transform-react (experimental); the fallback is
  // @rolldown/plugin-babel with reactCompilerPreset().
  plugins: [
    react({
      // The compiler skips node_modules on its own too, unless `sources` names the files.
      compiler: { sources: [resolve(import.meta.dirname, "src") + "/", designSystem] },
      exclude: /\/node_modules\/(?!@engawahq\/design-system\/)/,
    }),
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
  // The design system is not pre-bundled in dev: the pre-bundler runs no plugins, so it could not
  // resolve `~icons/…`. What it imports is.
  optimizeDeps: {
    exclude: ["@engawahq/design-system"],
    include: designSystemImports,
  },
  server: {
    proxy: {
      "/api": { target: api, changeOrigin: false, ws: true },
    },
  },
})
