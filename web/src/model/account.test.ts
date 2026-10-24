import { describe, expect, it } from "bun:test"

import {
  accessCounts,
  accessLayout,
  accessRows,
  filterAccess,
  projectName,
  type ProjectAccess,
} from "@/model/account"

const project = (
  name: string,
  role?: ProjectAccess["role"],
  displayName?: string,
): ProjectAccess => ({ name, role, displayName })

describe("account", () => {
  it("counts each role, and projects whose access could not be read apart", () => {
    const counts = accessCounts([
      project("web", "admin"),
      project("api", "developer"),
      project("docs", "developer"),
      project("billing"),
      project("legacy", "none"),
    ])
    expect(counts).toEqual({ admin: 1, developer: 2, viewer: 0, none: 1, unreadable: 1 })
  })

  it("lists unreadable projects first, then by role and name, and says the rest once", () => {
    const { rows, without } = accessRows([
      project("web", "viewer"),
      project("api", "admin"),
      project("legacy", "none", "Old site"),
      project("billing"),
      project("docs", "admin"),
    ])
    expect(rows.map((p) => p.name)).toEqual(["billing", "api", "docs", "web"])
    expect(without).toEqual({ named: ["Old site"], count: 1 })
  })

  it("only counts projects without a role once there are many", () => {
    const many = Array.from({ length: 8 }, (_, i) => project(`p${i}`, "none"))
    expect(accessRows(many)).toEqual({ rows: [], without: { named: [], count: 8 } })
  })

  it("is a list for a few projects and a filterable table for many", () => {
    expect(accessLayout(0)).toBe("empty")
    expect(accessLayout(1)).toBe("list")
    expect(accessLayout(10)).toBe("list")
    expect(accessLayout(11)).toBe("table")
  })

  it("filters by name and shown name", () => {
    const projects = [project("shop-web", "admin", "Storefront"), project("api", "viewer")]
    expect(filterAccess(projects, "store").map((p) => p.name)).toEqual(["shop-web"])
    expect(filterAccess(projects, "SHOP web").map((p) => p.name)).toEqual(["shop-web"])
    expect(filterAccess(projects, "  ")).toStrictEqual(projects)
    expect(projectName(project("api", "viewer", " "))).toBe("api")
  })
})
