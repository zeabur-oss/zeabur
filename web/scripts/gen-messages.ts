// Writes src/i18n/generated/<lang>.ts from the server sentences' catalogs in
// crates/zeabur-gaps/messages. `--check` writes nothing and fails when a file is out of date.
// The Vite plugin in vite.config.ts runs it on every build and when a catalog changes.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"

export const catalogs = resolve(import.meta.dirname, "../../crates/zeabur-gaps/messages")
const generated = resolve(import.meta.dirname, "../src/i18n/generated")
const languages = ["en", "zh-TW", "zh-CN"]

const source = (language: string) => {
  const { format, messages } = JSON.parse(
    readFileSync(resolve(catalogs, `${language}.json`), "utf8"),
  ) as { format: Record<string, string>; messages: Record<string, string> }
  const entries = (object: Record<string, string>, indent: string) =>
    Object.entries(object)
      .map(([key, value]) => `${indent}${JSON.stringify(key)}: ${JSON.stringify(value)},\n`)
      .join("")
  return (
    `// Generated from crates/zeabur-gaps/messages/${language}.json by scripts/gen-messages.ts.\n` +
    `import { sentences } from "@/i18n/template"\n\n` +
    `export default sentences({\n` +
    `  format: {\n${entries(format, "    ")}  },\n` +
    `  messages: {\n${entries(messages, "    ")}  },\n` +
    `})\n`
  )
}

/** Writes the files that differ from the catalogs (none with `check`); returns their names. */
export function generate(check = false): string[] {
  const stale: string[] = []
  for (const language of languages) {
    const file = resolve(generated, `${language}.ts`)
    const text = source(language)
    if (existsSync(file) && readFileSync(file, "utf8") === text) continue
    stale.push(`src/i18n/generated/${language}.ts`)
    if (!check) {
      mkdirSync(generated, { recursive: true })
      writeFileSync(file, text)
    }
  }
  return stale
}

if (import.meta.main) {
  const check = process.argv.includes("--check")
  const stale = generate(check)
  if (check && stale.length) {
    console.error(`out of date (run \`bun scripts/gen-messages.ts\` in web/): ${stale.join(", ")}`)
    process.exit(1)
  }
}
