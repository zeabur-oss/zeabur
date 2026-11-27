import { describe, expect, test } from "bun:test"

import type { Service } from "@/model/service"
import { groupChange, groupNames } from "@/model/service-group"

const service = (name: string, fields: Partial<Service> = {}) =>
  ({ kind: "Deployment", name, group: "default", ...fields }) as unknown as Service

describe("a service's group", () => {
  test("offers only the groups set by the group label", () => {
    const services = [
      service("web", { group: "shop", grouped: true }),
      service("db", { group: "primary" }),
      service("api", { group: "shop", grouped: true }),
      service("mail", { group: "admin", grouped: true }),
    ]
    expect(groupNames(services)).toStrictEqual(["admin", "shop"])
  })

  test("puts a service in a group, moves it, or takes it out", () => {
    const workload = { kind: "Deployment" as const, name: "web" }
    expect(groupChange(service("web"), " shop ")).toStrictEqual({
      op: "group",
      workload,
      group: "shop",
    })
    const grouped = service("web", { group: "shop", grouped: true })
    expect(groupChange(grouped, "admin")).toStrictEqual({ op: "group", workload, group: "admin" })
    expect(groupChange(grouped, "")).toStrictEqual({ op: "group", workload })
  })

  test("asks for nothing when the group stays or the name is not valid", () => {
    expect(groupChange(service("web", { group: "shop", grouped: true }), "shop")).toBeUndefined()
    expect(groupChange(service("db", { group: "primary" }), "")).toBeUndefined()
    expect(groupChange(service("web"), "my shop")).toBeUndefined()
  })
})
