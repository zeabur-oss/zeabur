import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { listenerServes, offered, parentOf } from "@/model/gateways"

describe("gateways", () => {
  const gateways: Schemas.GatewayView[] = [
    {
      namespace: "gateway",
      name: "public",
      listeners: [{ name: "https", hostname: "*.example.com" }, { name: "any" }],
    },
  ]

  test("a listener serves the hosts its hostname matches", () => {
    expect(listenerServes("*.example.com", "a.b.example.com")).toBe(true)
    expect(listenerServes("*.example.com", "example.com")).toBe(false)
    expect(listenerServes("shop.example.com", "www.example.com")).toBe(false)
    expect(listenerServes(undefined, "anything.test")).toBe(true)
  })

  test("only the listeners that serve the host are offered, and one is a parent", () => {
    expect(offered(gateways, "shop.example.org")).toEqual(["gateway/public/any"])
    expect(offered(gateways, "shop.example.com")).toEqual([
      "gateway/public/https",
      "gateway/public/any",
    ])
    expect(parentOf("gateway/public/https")).toEqual({
      namespace: "gateway",
      name: "public",
      sectionName: "https",
    })
    expect(parentOf("")).toBe(undefined)
  })
})
