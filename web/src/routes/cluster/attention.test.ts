import { describe, expect, test } from "bun:test"
import { createElement, Fragment } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import type { Release } from "@/model/update-check"
import type { Schemas } from "@/api/client"
import { componentsBehind, eolItem, skewItem, updateItem } from "@/routes/cluster/attention"
import { upgradeCommands } from "@/routes/cluster/zeabur-version"

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
    const item = updateItem(t, running, { latest: release("0.2.0"), security: [], health: "warn" })
    expect(item.health).toBe("warn")
    expect(item.text).toBe("Zeabur 0.2.0 is out")
    expect(item.detail).toBe("Run helm upgrade with the new chart version.")
    expect(item.lift).toBe(false)
    expect(item.problem).toBe(false)
  })

  test("a security release fails, names the fixed versions and goes to Home", () => {
    const fixes = [release("0.1.3"), release("0.1.2")]
    const item = updateItem(t, running, {
      latest: release("0.2.0"),
      security: fixes,
      health: "fail",
    })
    expect(item.health).toBe("fail")
    expect(item.text).toBe("Zeabur 0.1.1 has known security problems")
    expect(item.detail).toContain("Fixed in v0.1.3, v0.1.2.")
    expect(item.detail).toContain("helm upgrade")
    expect(item.lift).toBe(true)
  })

  test("a security release links the notes of each release that fixes it", () => {
    const fixes = [release("0.1.3"), release("0.1.2")]
    const item = updateItem(t, running, {
      latest: release("0.2.0"),
      security: fixes,
      health: "fail",
    })
    const html = renderToStaticMarkup(createElement(Fragment, null, item.link))
    expect(html).toContain('href="https://example.test/0.1.3"')
    expect(html).toContain('href="https://example.test/0.1.2"')
    expect(html).toContain("v0.1.2 release notes")
  })

  test("zeaburctl is named as itself and has no install step beyond its own", () => {
    const local = { ...running, installedBy: "zeaburctl" } as const
    const item = updateItem(t, local, { latest: release("0.2.0"), security: [], health: "warn" })
    expect(item.text).toBe("zeaburctl 0.2.0 is out")
    expect(item.detail).toBe("Update zeaburctl and run zeaburctl ui again.")
  })

  test("an unknown install method leaves out the install step", () => {
    const item = updateItem(
      t,
      { version: "0.1.1", updateCheck: true },
      {
        latest: release("0.2.0"),
        security: [],
        health: "warn",
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

describe("upgradeCommands", () => {
  test("give Helm's steps to the new version", () => {
    expect(upgradeCommands("helm", "0.2.0")[0]).toContain("--version 0.2.0")
    for (const by of ["argo-cd", "flux", "zeaburctl", undefined] as const)
      expect(upgradeCommands(by, "0.2.0")).toEqual([])
  })
})

const date = (d: string) => d
const upgrades = (patch: Partial<Schemas.Upgrades>) =>
  ({ minor: "1.30", attention: [], skew: [], ...patch }) as unknown as Schemas.Upgrades

describe("version items", () => {
  test("end of life comes from the server's finding, with its health", () => {
    const past = eolItem(
      t,
      date,
      upgrades({
        attention: [{ state: "end-of-life", health: "fail" }],
        release: { endOfLifeAt: "2025-06-28" } as Schemas.Upgrades["release"],
      }),
    )
    expect(past).toMatchObject({ health: "fail" })
    expect(past?.text).toContain("2025-06-28")
    const soon = eolItem(
      t,
      date,
      upgrades({
        attention: [{ state: "maintenance", health: "warn" }],
        release: { endOfLifeAt: "2026-12-28" } as Schemas.Upgrades["release"],
      }),
    )
    expect(soon).toMatchObject({ health: "warn" })
    expect(soon?.detail).toContain("After that it gets no security fixes.")
  })

  test("nothing to say without a finding", () => {
    expect(eolItem(t, date, upgrades({}))).toBe(undefined)
    expect(eolItem(t, date, undefined)).toBe(undefined)
    expect(skewItem(t, upgrades({}))).toBe(undefined)
  })

  test("kubelets past the supported skew are one item that names them", () => {
    const skew = [
      { node: "a", kubeletVersion: "v1.27.1", unsupported: true },
      { node: "b", kubeletVersion: "v1.27.1", unsupported: true },
      { node: "c", kubeletVersion: "v1.30.1", unsupported: false },
    ] as Schemas.Upgrades["skew"]
    const item = skewItem(t, upgrades({ skew }))
    expect(item?.nodes).toStrictEqual(["a", "b"])
    expect(item?.members?.map((m) => m.label)).toStrictEqual(["a", "b"])
    expect(skewItem(t, upgrades({ skew: skew.slice(0, 1) }))?.members).toBe(undefined)
  })
})
