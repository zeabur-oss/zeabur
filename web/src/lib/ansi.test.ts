import { describe, test, expect } from "bun:test"

import { parseAnsi } from "@/lib/ansi"
import { forAll } from "@/test/property"

const esc = "\u001b"

// Escape sequences a container's log may hold: colours (16, 256 and true colour), styles and
// resets, cursor and erase controls, a window title, a character set switch.
const escapes = [
  `${esc}[0m`,
  `${esc}[m`,
  `${esc}[1;31m`,
  `${esc}[4m`,
  `${esc}[22;39;49m`,
  `${esc}[92;104m`,
  `${esc}[38;5;196m`,
  `${esc}[48;2;10;20;30;3m`,
  `${esc}[2K`,
  `${esc}[1A`,
  `${esc}[?25l`,
  `${esc}]0;title\u0007`,
  `${esc}]8;;https://example.com${esc}\\`,
  `${esc}(B`,
  `${esc}7`,
  `${esc}c`,
]

describe("parseAnsi", () => {
  test("keeps a line's text exactly, as text, and drops every escape sequence", () => {
    forAll((random) => {
      const parts = random.list(10, () =>
        random.int(0, 1) === 0
          ? { text: random.pick(["plain", " ", "<b>bold</b>", "&amp;", "[1m", "日本"]) }
          : { escape: random.pick(escapes) },
      )
      const line = parts.map((p) => p.text ?? p.escape).join("")
      const text = parts.map((p) => p.text ?? "").join("")
      const segments = parseAnsi(line)
      expect(segments.map((s) => s.text).join("")).toBe(text)
      let start = 0
      for (const [i, segment] of segments.entries()) {
        // Keys are offsets into the text; no empty segment, no two alike in a row.
        expect(segment.start).toBe(start)
        expect(segment.text).not.toBe("")
        expect(segment.className).not.toBe(segments[i - 1]?.className)
        // Class names only: nothing a browser would read as markup or a style.
        expect(segment.className).toMatch(/^[a-z0-9 :/.-]*$/)
        start += segment.text.length
      }
    })
  })

  test("styles follow SGR: set, reset, and colour arguments skipped", () => {
    expect(parseAnsi(`${esc}[31mred${esc}[0m plain`)).toStrictEqual([
      { start: 0, text: "red", className: "text-red-700 dark:text-red-400" },
      { start: 3, text: " plain", className: "" },
    ])
    // 256-colour and true-colour arguments are consumed; what follows them still applies.
    expect(parseAnsi(`${esc}[38;5;196;1mx`)).toStrictEqual([
      { start: 0, text: "x", className: "font-bold" },
    ])
    expect(parseAnsi(`${esc}[48;2;1;2;3;4mx${esc}[24my`)).toStrictEqual([
      { start: 0, text: "x", className: "underline" },
      { start: 1, text: "y", className: "" },
    ])
    // What `tput sgr0` writes ends a style like a plain reset.
    expect(parseAnsi(`${esc}[1mbold${esc}(B${esc}[mtext`)).toStrictEqual([
      { start: 0, text: "bold", className: "font-bold" },
      { start: 4, text: "text", className: "" },
    ])
  })
})
