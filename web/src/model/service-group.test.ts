import { describe, expect, test } from "bun:test"

import type { Service } from "@/model/service"
import { groupChange, groupMoves, groupSections, placementOf } from "@/model/service-group"

const service = (name: string, fields: Partial<Service> = {}) =>
  ({
    kind: "Deployment",
    name,
    group: "default",
    ...fields,
  }) as unknown as Service

const names = (sections: ReturnType<typeof groupSections>) =>
  sections.map((s) => [s.name, s.services.map((x) => x.name)])

describe("a service's group", () => {
  test("puts a service in a group, moves it, or takes it out", () => {
    const workload = { kind: "Deployment" as const, name: "web" }
    expect(groupChange(service("web"), " shop ")).toStrictEqual({
      op: "group",
      workload,
      group: "shop",
    })
    const grouped = service("web", { group: "shop", grouped: true })
    expect(groupChange(grouped, "admin")).toStrictEqual({
      op: "group",
      workload,
      group: "admin",
    })
    expect(groupChange(grouped, "")).toStrictEqual({ op: "group", workload })
  })

  test("asks for nothing when the group stays or the name is not valid", () => {
    expect(groupChange(service("web", { group: "shop", grouped: true }), "shop")).toBeUndefined()
    expect(groupChange(service("db", { group: "primary" }), "")).toBeUndefined()
    expect(groupChange(service("web"), "my shop")).toBeUndefined()
  })
})

describe("the group dialog", () => {
  const services = [
    service("web", { group: "shop", grouped: true }),
    service("db", { group: "primary" }),
    service("api", { group: "shop", grouped: true }),
    service("mail", { group: "admin", grouped: true }),
  ]

  test("lists the groups by name, the services in no group last", () => {
    expect(names(groupSections(services, placementOf(services)))).toStrictEqual([
      ["admin", ["mail"]],
      ["shop", ["api", "web"]],
      ["", ["db"]],
    ])
  })

  test("keeps a new group with no service in its place by name", () => {
    expect(names(groupSections(services, placementOf(services), ["billing"]))).toStrictEqual([
      ["admin", ["mail"]],
      ["billing", []],
      ["shop", ["api", "web"]],
      ["", ["db"]],
    ])
  })

  test("drops a group whose last service moves out", () => {
    const placement = { ...placementOf(services), "Deployment/mail": "shop" }
    expect(names(groupSections(services, placement))).toStrictEqual([
      ["shop", ["api", "mail", "web"]],
      ["", ["db"]],
    ])
  })

  test("asks for one change for each service moved, none for the rest", () => {
    const placement = {
      ...placementOf(services),
      "Deployment/db": "billing",
      "Deployment/web": "",
      "Deployment/mail": "billing",
    }
    expect(groupMoves(services, placement)).toStrictEqual([
      { op: "group", workload: { kind: "Deployment", name: "web" } },
      {
        op: "group",
        workload: { kind: "Deployment", name: "db" },
        group: "billing",
      },
      {
        op: "group",
        workload: { kind: "Deployment", name: "mail" },
        group: "billing",
      },
    ])
  })

  test("asks for nothing when every service is back where it was", () => {
    const moved = { ...placementOf(services), "Deployment/web": "admin" }
    expect(groupMoves(services, { ...moved, "Deployment/web": "shop" })).toStrictEqual([])
  })
})
