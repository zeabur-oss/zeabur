import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { resolve } from "node:path"

import { locales } from "@/i18n"
import {
  completeLocales,
  sourceHash,
  summaries,
  summaryKey,
  summaryOf,
} from "@/i18n/component-summaries"

const bundle = resolve(import.meta.dir, "../../../crates/zeabur-catalog/bundle/components")

/** `capability/id` to the description, trimmed as the server sends it. */
const catalogue = new Map<string, string>(
  readdirSync(bundle).flatMap((capability) =>
    readdirSync(resolve(bundle, capability))
      .filter((file) => file.endsWith(".yaml"))
      .map((file) => {
        const { metadata } = Bun.YAML.parse(
          readFileSync(resolve(bundle, capability, file), "utf8"),
        ) as { metadata: { description: string } }
        return [
          `${capability}/${file.slice(0, -".yaml".length)}`,
          metadata.description.trim(),
        ] as const
      }),
  ),
)

const keyOf = (component: string) => {
  const [capability = "", id = ""] = component.split("/")
  return summaryKey(capability, id, catalogue.get(component) ?? "")
}

describe("component summaries", () => {
  test("the catalogue is there to check against", () => {
    expect(catalogue.size).toBeGreaterThan(10)
  })

  test("no summary is for a component the catalogue no longer has", () => {
    const orphans = Object.keys(summaries).filter((key) => !catalogue.has(key.split("#")[0] ?? ""))
    expect(orphans).toEqual([])
  })

  test("no summary is for a description that has since changed", () => {
    const stale = Object.keys(summaries)
      .filter((key) => catalogue.has(key.split("#")[0] ?? ""))
      .filter((key) => key !== keyOf(key.split("#")[0] ?? ""))
      .map((key) => `${key} is now ${keyOf(key.split("#")[0] ?? "")}`)
    expect(stale).toEqual([])
  })

  test("every component has a summary in each complete locale", () => {
    const missing = [...catalogue.keys()].flatMap((component) =>
      completeLocales
        .filter((locale) => !summaries[keyOf(component)]?.[locale])
        .map((locale) => `${component} ${locale}`),
    )
    expect(missing).toEqual([])
  })

  test("a summary is one sentence", () => {
    for (const summary of Object.values(summaries))
      for (const locale of locales) {
        const text = summary[locale]
        if (text) expect(text.split(/[.。](?!$)/).length).toBe(1)
      }
  })

  test("a locale without its sentence falls back to English, a changed description to none", () => {
    const key = Object.keys(summaries)[0] ?? ""
    const component = key.split("#")[0] ?? ""
    const [capability = "", id = ""] = component.split("/")
    const description = catalogue.get(component) ?? ""
    const summary = summaries[key]
    if (!summary) throw new Error("no summaries")
    const written = summary["zh-CN"]
    delete summary["zh-CN"]
    try {
      expect(summaryOf(capability, id, description, "zh-CN")).toBe(summary.en)
    } finally {
      summary["zh-CN"] = written
    }
    expect(summaryOf(capability, id, `${description} More.`, "en")).toBe(undefined)
  })

  test("the hash is FNV-1a over UTF-8", () => {
    expect(sourceHash("")).toBe("811c9dc5")
    expect(sourceHash("a")).toBe("e40c292c")
    expect(sourceHash("é")).not.toBe(sourceHash("e"))
  })
})
