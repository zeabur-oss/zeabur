// Prints every zh-TW user string after its key, for scripts/check-zh-tw.sh: the catalogue in
// src/i18n/zh-TW.ts (with the server's sentences, `sentence.*`) and the zh-TW texts of the
// server's sign-in pages (crates/engawa/src/pages.rs). zh-CN is never printed.
//
// Each string is one line and a paragraph of its own (a blank line between them), so the linter
// reads no sentence across two strings; the key is in brackets, which it does not read as a word.
// A function value is called with its parameters' names as placeholders (`<service>`: Latin, which
// the linter leaves alone), so it prints one of its forms; a server sentence gets every parameter
// it reads, as its name. The catalogues are imported by a
// computed path: this file is type-checked with tsconfig.node.json, which does not resolve the
// interface's `@/` imports.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

type Message = (...args: unknown[]) => unknown

const load = async (file: string) =>
  (await import(resolve(import.meta.dirname, "../src/i18n", file))) as Record<string, unknown>

/** A parameter's placeholder: its name in text, 2 in arithmetic and comparisons. */
const placeholder = (name: string) => ({
  [Symbol.toPrimitive]: (hint: string) => (hint === "number" ? 2 : `<${name}>`),
})
/** The parameter names of an arrow function, from its source. */
const parameters = (f: Message) =>
  (/^\(?([^)=]*)\)?\s*=>/.exec(f.toString())?.[1] ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean)
// A function that takes a list (`common.list`, `confirm.resources`) gets one, of pairs first: a
// list of names would be read as pairs of letters.
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
const params = new Proxy(
  {},
  { get: (_, key) => (typeof key === "string" ? `<${key}>` : undefined) },
)
for (const [key, value] of Object.entries(zhTW)) {
  if (typeof value === "string") print(key, value)
  else print(key, key.startsWith("sentence.") ? String(value(params)) : call(value))
}

// The sign-in pages: the string on a `Lang::ZhTw` arm, on its line or the next.
const pages = readFileSync(
  resolve(import.meta.dirname, "../../crates/engawa/src/pages.rs"),
  "utf8",
).split("\n")
pages.forEach((line, index) => {
  const arm = /Lang::ZhTw\)? => (?:"([^"]*)"|\{$)/.exec(line)
  const text = arm && (arm[1] ?? /^\s*"([^"]*)"/.exec(pages[index + 1])?.[1])
  if (text) print(`pages.rs:${index + 1}`, text)
})

console.log(lines.join("\n\n"))
