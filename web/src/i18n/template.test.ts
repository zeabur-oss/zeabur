import { describe, test, expect } from "bun:test"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import { en, type Messages } from "@/i18n/en"
import { parse, render, type Params } from "@/i18n/template"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"

// The sentences zeabur_gaps::messages renders too, from the same catalogs.
const examples = JSON.parse(
  readFileSync(
    resolve(import.meta.dir, "../../../crates/zeabur-gaps/messages/examples.json"),
    "utf8",
  ),
) as ({ key: string; params: Params } & Record<"en" | "zh-TW" | "zh-CN", string>)[]

const dictionaries = { en, "zh-TW": zhTW, "zh-CN": zhCN } as Record<string, Messages>

describe("server sentences", () => {
  test("each language words the examples as the server does", () => {
    expect(examples).not.toEqual([])
    for (const example of examples) {
      for (const [language, messages] of Object.entries(dictionaries)) {
        const sentence = messages[`sentence.${example.key}` as keyof Messages] as (
          p: Params,
        ) => string
        expect(`${language} ${example.key}: ${sentence(example.params)}`).toBe(
          `${language} ${example.key}: ${example[language as "en"]}`,
        )
      }
    }
  })

  test("a plural writes its count where # is, and nowhere else", () => {
    const format = { listSeparator: ", ", listAnd: " and ", seconds: "${n}s", minutes: "${n}m" }
    const template = "#${n, plural, one {# try} other {# tries}}"
    expect(render(template, { n: "1" }, format)).toBe("#1 try")
    expect(render(template, { n: "3" }, format)).toBe("#3 tries")
    expect(render("${s, duration} or ${t, duration}", { s: "120", t: "90" }, format)).toBe(
      "2m or 90s",
    )
  })

  test("a template that is not well-formed is refused", () => {
    expect(() => parse("${n, select, a {x}")).toThrow()
    expect(() => parse("${n")).toThrow()
  })
})
