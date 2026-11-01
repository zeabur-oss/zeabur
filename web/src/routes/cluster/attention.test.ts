import { describe, expect, test } from "bun:test"
import { createElement, Fragment } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import type { Release } from "@/model/update-check"
import { componentsBehind, updateItem } from "@/routes/cluster/attention"

const t = translator(en)
const release = (version: string): Release => ({
  version,
  url: `https://example.test/${version}`,
  prerelease: false,
  security: false,
})
const running = { version: "0.1.1", updateCheck: true, installedBy: "helm" } as const

describe("updateItem", () => {
  test("a normal release is a warning that stays off Home", () => {
    const item = updateItem(t, "en", running, { latest: release("0.2.0"), security: [] })
    expect(item.health).toBe("warn")
    expect(item.text).toBe("Zeabur 0.2.0 is out")
    expect(item.detail).toBe("Run helm upgrade with the new chart version.")
    expect(item.lift).toBe(false)
    expect(item.problem).toBe(false)
  })

  test("a security release fails, names the fixed versions and goes to Home", () => {
    const fixes = [release("0.1.3"), release("0.1.2")]
    const item = updateItem(t, "en", running, { latest: release("0.2.0"), security: fixes })
    expect(item.health).toBe("fail")
    expect(item.text).toBe("Zeabur 0.1.1 has known security problems")
    expect(item.detail).toContain("Fixed in v0.1.3, v0.1.2.")
    expect(item.detail).toContain("helm upgrade")
    expect(item.lift).toBe(true)
  })

  test("a security release links the notes of each release that fixes it", () => {
    const fixes = [release("0.1.3"), release("0.1.2")]
    const item = updateItem(t, "en", running, { latest: release("0.2.0"), security: fixes })
    const html = renderToStaticMarkup(createElement(Fragment, null, item.link))
    expect(html).toContain('href="https://example.test/0.1.3"')
    expect(html).toContain('href="https://example.test/0.1.2"')
    expect(html).toContain("v0.1.2 release notes")
  })

  test("zeaburctl is named as itself and has no install step beyond its own", () => {
    const local = { ...running, installedBy: "zeaburctl" } as const
    const item = updateItem(t, "en", local, { latest: release("0.2.0"), security: [] })
    expect(item.text).toBe("zeaburctl 0.2.0 is out")
    expect(item.detail).toBe("Update zeaburctl and run zeaburctl ui again.")
  })

  test("an unknown install method leaves out the install step", () => {
    const item = updateItem(
      t,
      "en",
      { version: "0.1.1", updateCheck: true },
      {
        latest: release("0.2.0"),
        security: [],
      },
    )
    expect(item.detail).toBe("")
  })
})

describe("componentsBehind", () => {
  test("nothing behind is no item", () => {
    expect(componentsBehind({ items: [{ installations: [] }] }, t)).toBeUndefined()
    expect(componentsBehind(undefined, t)).toBeUndefined()
  })
})
