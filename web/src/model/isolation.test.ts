import { describe, expect, test } from "bun:test"

import { allowFrom, allowName, projectPolicy } from "@/model/isolation"

describe("isolation", () => {
  test("the project policy lets in the project, the platform and kube-system", () => {
    const from = projectPolicy("shop").spec.ingress[0]?.from
    expect(from).toEqual([
      { podSelector: {} },
      { namespaceSelector: { matchLabels: { "installer.zeabur.com/baseline": "true" } } },
      { namespaceSelector: { matchLabels: { "kubernetes.io/metadata.name": "kube-system" } } },
    ])
  })

  test("an allowed project is chosen by its immutable name label", () => {
    const policy = allowFrom("shop", "blog")
    expect(policy.metadata).toMatchObject({ name: "zeabur-allow-from-blog", namespace: "shop" })
    expect(policy.spec.ingress[0]?.from).toEqual([
      { namespaceSelector: { matchLabels: { "kubernetes.io/metadata.name": "blog" } } },
    ])
  })

  test("a long name stays a valid name", () => {
    const name = allowName(`${"a".repeat(44)}-b`)
    expect(name.length).toBeLessThanOrEqual(63)
    expect(name).not.toMatch(/-$/)
  })
})
