// Fails on any use of a declaration marked `@deprecated` (TypeScript suggestions 6385 and 6387).
// Usage: bun scripts/check-deprecated.ts <project dir>... Each directory has a tsconfig.json; its
// project references are followed, and its own typescript package is used.
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"
import type * as TS from "typescript/unstable/async"

const deprecated = new Set([6385, 6387])
let found = 0

function references(config: string): string[] {
  const { references = [] } = Bun.JSONC.parse(readFileSync(config, "utf8")) as {
    references?: { path: string }[]
  }
  return references.map((r) => {
    const target = path.resolve(path.dirname(config), r.path)
    return target.endsWith(".json") ? target : path.join(target, "tsconfig.json")
  })
}

for (const arg of process.argv.slice(2)) {
  const dir = path.resolve(arg)
  const resolve = createRequire(path.join(dir, "package.json")).resolve
  const ts = (await import(resolve("typescript/unstable/async"))) as typeof TS

  const configs = new Set<string>()
  const pending = [path.join(dir, "tsconfig.json")]
  for (let file = pending.pop(); file !== undefined; file = pending.pop()) {
    if (configs.has(file)) continue
    configs.add(file)
    pending.push(...references(file))
  }

  const api = new ts.API({ cwd: dir })
  const snapshot = await api.updateSnapshot({ openProjects: [...configs] })
  for (const project of snapshot.getProjects()) {
    const roots = new Set(project.rootFiles.filter((f) => !f.endsWith(".d.ts")))
    if (roots.size === 0) continue
    for (const d of await project.program.getSuggestionDiagnostics()) {
      if (!deprecated.has(d.code) || d.fileName === undefined || !roots.has(d.fileName)) continue
      const file = await project.program.getSourceFile(d.fileName)
      const { line, character } = file?.getLineAndCharacterOfPosition(d.pos) ?? {
        line: 0,
        character: 0,
      }
      console.error(
        `${path.relative(process.cwd(), d.fileName)}:${line + 1}:${character + 1}: ${d.text}`,
      )
      found += 1
    }
  }
  await api.close()
}

if (found > 0) {
  console.error(`${found} use(s) of deprecated declarations`)
  process.exit(1)
}
