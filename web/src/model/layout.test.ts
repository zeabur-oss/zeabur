import { describe, expect, test } from "bun:test"

import { fittedZoom, layout, mapReadable } from "@/model/layout"
import type { Service } from "@/model/service"

const service = (name: string, fields: Partial<Service> = {}) =>
  ({
    id: `projects/shop/services/deployment/${name}`,
    kind: "Deployment",
    name,
    pods: [],
    containers: [],
    endpointNames: [],
    routeNames: [],
    status: { health: "ok", word: "running" },
    ...fields,
  }) as unknown as Service

const many = (n: number) => Array.from({ length: n }, (_, i) => service(`s-${i}`))

const columns = (services: Service[]) =>
  new Set(layout(services, []).nodes.map((node) => node.x)).size

describe("the map's layout", () => {
  test("draws no domain cards: a service card names its host", () => {
    const laid = layout([service("api", { endpointNames: ["api"], routeNames: ["api"] })], [])
    expect(laid.nodes.map((n) => n.id)).toStrictEqual(["Deployment/api"])
    expect(laid.frames.map((f) => f.lane)).toStrictEqual(["web"])
  })

  test("keeps a lane of up to four cards in one column", () => {
    expect(columns(many(1))).toBe(1)
    expect(columns(many(4))).toBe(1)
  })

  test("wraps a taller lane into two, then at most three columns", () => {
    expect(columns(many(5))).toBe(2)
    expect(columns(many(9))).toBe(3)
    expect(columns(many(30))).toBe(3)
  })

  test("never puts two cards on one spot", () => {
    const spots = layout(many(20), []).nodes.map((n) => `${n.x},${n.y}`)
    expect(new Set(spots).size).toBe(20)
  })
})

describe("whether the map is shown first", () => {
  const canvas = [1200, 600] as const

  test("not for an empty project", () => {
    expect(mapReadable(layout([], []), ...canvas)).toBe(false)
  })

  test("for one or a few services", () => {
    expect(mapReadable(layout(many(1), []), ...canvas)).toBe(true)
    expect(mapReadable(layout(many(6), []), ...canvas)).toBe(true)
  })

  test("not when the fitted map would be too small to read", () => {
    const laid = layout(many(12), [])
    expect(fittedZoom(laid, 600, 400)).toBeLessThan(0.75)
    expect(mapReadable(laid, 600, 400)).toBe(false)
  })

  test("not for dozens of services, however large the screen", () => {
    expect(mapReadable(layout(many(48), []), 4000, 3000)).toBe(false)
  })
})
