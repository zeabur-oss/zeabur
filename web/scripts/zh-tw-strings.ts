// Prints every zh-TW user string after its key, for scripts/check-zh-tw.sh. Each string is its own
// paragraph so the linter reads no sentence across two; placeholders are Latin (`<service>`), which
// it leaves alone. Imported by a computed path: tsconfig.node.json does not resolve `@/`.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

type Message = (...args: unknown[]) => unknown

const load = async (file: string) =>
  (await import(resolve(import.meta.dirname, "../src/i18n", file))) as Record<string, unknown>

/** Its name in text, 2 in arithmetic and comparisons. */
const placeholder = (name: string) => ({
  [Symbol.toPrimitive]: (hint: string) => (hint === "number" ? 2 : `<${name}>`),
})
const parameters = (f: Message) =>
  (/^\(?([^)=]*)\)?\s*=>/.exec(f.toString())?.[1] ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean)
// Pairs first: a list of names would be destructured as pairs of letters.
const attempts: ((name: string) => unknown)[] = [
  (name: string) => placeholder(name),
  (name: string) => [[`<${name}>`, "<value>"]],
  (name: string) => [`<${name}>`, "<other>"],
]
const call = (f: Message) => {
  for (const attempt of attempts) {
    try {
      return String(f(...parameters(f).map(attempt)))
    } catch {}
  }
  throw new Error(`no placeholder fits ${f}`)
}

const lines: string[] = []
const print = (key: string, text: string) => lines.push(`[${key}] ${text.replace(/\s+/g, " ")}`)

const { zhTW } = (await load("zh-TW.ts")) as { zhTW: Record<string, string | Message> }
// A sentence is called with several parameter sets, so each of its branches is printed.
const everyParam = (text: (name: string) => string | undefined): Record<string, string> =>
  new Proxy({}, { get: (_, name) => (typeof name === "string" ? text(name) : undefined) })
const sentenceParams = [
  everyParam((name) => `<${name}>`),
  ...["true", "false", "0", "1", "120", "Secret"].map((value) => everyParam(() => value)),
  {},
]
for (const [key, value] of Object.entries(zhTW)) {
  if (typeof value === "string") print(key, value)
  else if (key.startsWith("sentence."))
    for (const text of new Set(sentenceParams.map((params) => String(value(params)))))
      print(key, text)
  else print(key, call(value))
}

// The sign-in pages: the string on a `Lang::ZhTw` arm, on its line or the next.
const pages = readFileSync(
  resolve(import.meta.dirname, "../../crates/zeabur/src/pages.rs"),
  "utf8",
).split("\n")
pages.forEach((line, index) => {
  const arm = /Lang::ZhTw\)? => (?:"([^"]*)"|\{$)/.exec(line)
  const text = arm && (arm[1] ?? /^\s*"([^"]*)"/.exec(pages[index + 1])?.[1])
  if (text) print(`pages.rs:${index + 1}`, text)
})

console.log(lines.join("\n\n"))
