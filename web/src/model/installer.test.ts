import { describe, expect, test } from "bun:test"

import { needed } from "@/model/installer"

const component = (id: string, displayName: string, defaults: Record<string, string>) =>
  ({ id, displayName, defaults }) as never

const capabilities = [
  {
    name: "databases",
    components: [
      component("moco", "MOCO", { "single-node": "optional" }),
      component("cloudnative-pg", "CloudNativePG", { "single-node": "recommended" }),
    ],
  },
  { name: "image-updates", components: [component("keel", "Keel", { "single-node": "optional" })] },
] as never

describe("what to install", () => {
  test("a capability becomes the component the catalogue recommends for it", () => {
    expect(needed("databases", capabilities)).toEqual({
      id: "databases/cloudnative-pg",
      name: "CloudNativePG",
    })
  })

  test("without a recommended one, the first", () => {
    expect(needed("image-updates", capabilities)).toEqual({
      id: "image-updates/keel",
      name: "Keel",
    })
  })

  test("a component keeps its id", () => {
    expect(needed("databases/moco", capabilities)).toEqual({ id: "databases/moco", name: "MOCO" })
  })

  test("an unknown capability stays as it is", () => {
    expect(needed("builds", undefined)).toEqual({ id: "builds", name: "builds" })
  })
})
