import { readdirSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import Icons from "unplugin-icons/vite"
import { defineConfig } from "vite"

// The pages the app bundles (tauri.conf.json `frontendDist`): the chooser, and the page a cluster
// window shows while its local service starts or when it cannot. The cluster's web interface
// itself comes from the local service.

// The design system ships sources, compiled like the app's own (React Compiler, `~icons/…`), as in
// web/vite.config.ts.
const designSystem = "/node_modules/@zeabur-oss/design-system/"
const designSystemSources = resolve(
  import.meta.dirname,
  "node_modules/@zeabur-oss/design-system/src",
)
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
    react({
      compiler: { sources: [resolve(import.meta.dirname, "src") + "/", designSystem] },
      exclude: /\/node_modules\/(?!@zeabur-oss\/design-system\/)/,
    }),
    tailwindcss(),
    Icons({ scale: 1, compiler: "jsx", jsx: "react" }),
  ],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  // `tauri dev` loads the dev server at this port (tauri.conf.json `devUrl`).
  server: { port: 5174, strictPort: true },
  build: {
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
    sourcemap: false,
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, "index.html"),
        cluster: resolve(import.meta.dirname, "cluster.html"),
      },
    },
  },
  optimizeDeps: {
    exclude: ["@zeabur-oss/design-system"],
    include: designSystemImports,
  },
})
