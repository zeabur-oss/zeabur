// Loaded before the unit tests (bunfig.toml): `~icons/ph/<name>` built by unplugin-icons with the
// options of vite.config.ts, so components with icons render under `bun test`. Bun's runtime
// plugins cannot resolve a bare specifier by pattern (unplugin's Bun adapter relies on that), so
// every icon is registered by name and built by the plugin's own hooks when imported.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import Icons from "unplugin-icons"

const icons = Icons.raw({ scale: 1, compiler: "jsx", jsx: "react" }, { framework: "bun" })
const resolveId = icons.resolveId as (id: string) => string
const load = icons.load as (
  this: { addWatchFile(path: string): void },
  id: string,
) => Promise<{ code: string }>

const set = JSON.parse(
  readFileSync(resolve(import.meta.dirname, "../node_modules/@iconify-json/ph/icons.json"), "utf8"),
) as { icons: Record<string, unknown>; aliases?: Record<string, unknown> }

Bun.plugin({
  name: "unplugin-icons (test)",
  setup(build) {
    for (const name of [...Object.keys(set.icons), ...Object.keys(set.aliases ?? {})]) {
      const id = `~icons/ph/${name}`
      build.module(id, async () => ({
        contents: (await load.call({ addWatchFile() {} }, resolveId(id))).code,
        loader: "jsx",
      }))
    }
  },
})
