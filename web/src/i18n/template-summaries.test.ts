import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import {
  completeLocales,
  templateSummaries,
  templateSummary,
  templateSummaryKey,
} from "@/i18n/template-summaries"

/** The embedded catalog's index.json; the Nix dev shell sets the directory. */
const index = resolve(process.env.ZEABUR_CATALOG_RELEASE ?? "", "index.json")

const catalogue = new Map<string, string>(
  (
    JSON.parse(readFileSync(index, "utf8")) as {
      templates: { id: string; metadata: { description: string } }[]
    }
  ).templates.map(({ id, metadata }) => [id, metadata.description.trim()] as const),
)

const keyOf = (id: string) => templateSummaryKey(id, catalogue.get(id) ?? "")

describe("template summaries", () => {
  test("no summary is for a template the catalogue no longer has or a changed description", () => {
    const stale = Object.keys(templateSummaries).filter(
      (key) => key !== keyOf(key.split("#")[0] ?? ""),
    )
    expect(stale).toEqual([])
  })

  test("every template has a summary in each complete locale", () => {
    const missing = [...catalogue.keys()].flatMap((id) =>
      completeLocales
        .filter((locale) => !templateSummaries[keyOf(id)]?.[locale])
        .map((locale) => `${id} ${locale}`),
    )
    expect(missing).toEqual([])
  })

  test("a summary is one sentence", () => {
    for (const summary of Object.values(templateSummaries))
      for (const text of Object.values(summary)) expect(text.split(/[.。](?!$)/).length).toBe(1)
  })

  test("a changed description shows the catalogue's own words", () => {
    const description = catalogue.get("miniflux") ?? ""
    expect(templateSummary("miniflux", description, "zh-TW")).toBe("RSS 閱讀器，搭配 PostgreSQL。")
    expect(templateSummary("miniflux", `${description} More.`, "zh-TW")).toBe(
      `${description} More.`,
    )
  })
})
