import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import { aroundName, awaitsName, typedName } from "@/model/typed-name"

type Change = Schemas.Change

const api = { kind: "Deployment", name: "api" } as const
const policies = { group: "networking.k8s.io", version: "v1", resource: "networkpolicies" }
const remove: Change = { op: "delete", resource: policies, name: "zeabur-isolate-api" }

describe("typed confirmation", () => {
  test("a deletion waits for the object's name; other plans do not", () => {
    expect(typedName([remove])).toBe("zeabur-isolate-api")
    expect(awaitsName([remove], "")).toBe(true)
    expect(awaitsName([remove], "zeabur-isolate")).toBe(true)
    expect(awaitsName([remove], "ZEABUR-ISOLATE-API")).toBe(true)
    expect(awaitsName([remove], " zeabur-isolate-api ")).toBe(false)
    expect(awaitsName([{ op: "restart", workload: api }], "")).toBe(false)
    expect(typedName([{ op: "restart", workload: api }])).toBe(undefined)
  })

  test("each language's prompt has one place for the name, whatever the name is", () => {
    for (const messages of [en, zhTW, zhCN]) {
      for (const name of ["web", "to", "e", "輸入"]) {
        const [before, after] = aroundName(messages["plan.typeName"])
        expect(`${before}${name}${after}`).toBe(messages["plan.typeName"](name))
      }
    }
  })

  test.each(["deleteProject", "forceDeleteProject"] as const)(
    "%s waits for the project's name",
    (op) => {
      const deletion = [{ op }]
      expect(awaitsName(deletion, "", "shop")).toBe(true)
      expect(awaitsName(deletion, "shop", "shop")).toBe(false)
    },
  )
})
