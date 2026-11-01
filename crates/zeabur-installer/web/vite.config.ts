import { readdirSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import Icons from "unplugin-icons/vite"
import { defineConfig } from "vite"
import { devApi } from "./scripts/dev-api.ts"

// The design system ships sources, compiled like this app's own (React Compiler, `~icons/…`).
const designSystem = "/node_modules/@zeabur-oss/design-system/"

// The design system's imports, pre-bundled by name: some reach CommonJS the browser cannot load
// (`use-sync-external-store/shim`).
const designSystemSources = resolve(import.meta.dirname, "node_modules/@zeabur-oss/design-system/src")
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
  plugins: [
    devApi,
    react({
      // The compiler skips node_modules on its own too, unless `sources` names the files.
      compiler: { sources: [resolve(import.meta.dirname, "src") + "/", designSystem] },
      exclude: /\/node_modules\/(?!@zeabur\/design-system\/)/,
    }),
    tailwindcss(),
    Icons({ scale: 1, compiler: "jsx", jsx: "react" }),
  ],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  build: {
    // Every supported browser has <link rel="modulepreload">.
    modulePreload: { polyfill: false },
    assetsDir: "assets",
    // Fonts stay files, never data: URLs.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  // Not pre-bundled: the pre-bundler runs no plugins, so it cannot resolve `~icons/…`.
  optimizeDeps: {
    exclude: ["@zeabur-oss/design-system"],
    include: designSystemImports,
  },
})
