import { describe, test, expect } from "bun:test"

import { objectKey, parseObjectKey } from "@/lib/objects"
import { forAll } from "@/test/property"

// `?object=` is a link anyone can share or bookmark: every address must come back from its key.
describe("objectKey", () => {
  test("reads back as the address it was made from, core group included", () => {
    forAll((random) => {
      const address = {
        group: random.pick(["", "apps", "networking.k8s.io", "cert-manager.io"]),
        version: random.pick(["v1", "v1beta1", "v2alpha3"]),
        resource: random.pick(["configmaps", "deployments", "certificates"]),
        name: random.pick(["api", "shop-config", "a.b-c", "x1"]),
      }
      const key = objectKey(address)
      expect(parseObjectKey(key)).toStrictEqual(address)
      expect(parseObjectKey(`projects/shop/objects/${key}`)).toStrictEqual(address)
    })
  })

  test("a key of another shape reads as no address", () => {
    for (const key of ["", "core/v1/configmaps", "core/v1/configmaps/a/b", "core//configmaps/a"])
      expect(parseObjectKey(key), key).toBe(undefined)
  })
})
