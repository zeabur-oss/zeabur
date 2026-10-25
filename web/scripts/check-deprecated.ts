// Fails on any use of a declaration marked `@deprecated` (TypeScript suggestions 6385 and 6387).
// Usage: bun scripts/check-deprecated.ts <project dir>... Each directory has a tsconfig.json; its
// project references are followed, and its own typescript package is used.
import { createRequire } from "node:module"
import path from "node:path"
import type * as TS from "typescript"

const deprecated = new Set([6385, 6387])
let found = 0

for (const arg of process.argv.slice(2)) {
  const dir = path.resolve(arg)
  const ts = createRequire(path.join(dir, "package.json"))("typescript") as typeof TS
  const host: TS.ParseConfigFileHost = {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (d) => {
      throw new Error(ts.flattenDiagnosticMessageText(d.messageText, " "))
    },
  }

  const projects = new Map<string, TS.ParsedCommandLine>()
  const pending = [path.join(dir, "tsconfig.json")]
  for (let file = pending.pop(); file !== undefined; file = pending.pop()) {
    if (projects.has(file)) continue
    const config = ts.getParsedCommandLineOfConfigFile(file, {}, host)
    if (!config) throw new Error(`cannot read ${file}`)
    projects.set(file, config)
    for (const r of config.projectReferences ?? []) pending.push(ts.resolveProjectReferencePath(r))
  }

  for (const config of projects.values()) {
    const names = config.fileNames.filter((f) => !f.endsWith(".d.ts"))
    if (names.length === 0) continue
    const service = ts.createLanguageService({
      getScriptFileNames: () => names,
      getScriptVersion: () => "1",
      getScriptSnapshot: (f) => {
        const text = ts.sys.readFile(f)
        return text === undefined ? undefined : ts.ScriptSnapshot.fromString(text)
      },
      getCurrentDirectory: () => dir,
      getCompilationSettings: () => config.options,
      getDefaultLibFileName: (o) => ts.getDefaultLibFilePath(o),
      fileExists: ts.sys.fileExists,
      readFile: ts.sys.readFile,
      readDirectory: ts.sys.readDirectory,
      directoryExists: ts.sys.directoryExists,
      getDirectories: ts.sys.getDirectories,
    })
    for (const name of names) {
      for (const d of service.getSuggestionDiagnostics(name)) {
        if (!deprecated.has(d.code)) continue
        const { line, character } = d.file?.getLineAndCharacterOfPosition(d.start ?? 0) ?? {
          line: 0,
          character: 0,
        }
        const where = `${path.relative(process.cwd(), name)}:${line + 1}:${character + 1}`
        console.error(`${where}: ${ts.flattenDiagnosticMessageText(d.messageText, " ")}`)
        found += 1
      }
    }
  }
}

if (found > 0) {
  console.error(`${found} use(s) of deprecated declarations`)
  process.exit(1)
}
