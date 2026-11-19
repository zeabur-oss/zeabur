import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { undoOf, type Pods, type PodsOf } from "@/model/undo"

type Change = Schemas.Change

const api = { kind: "Deployment", name: "api" } as const
const pods =
  (known: Record<string, Pods>): PodsOf =>
  (workload) =>
    known[workload.name]

const policies = { group: "networking.k8s.io", version: "v1", resource: "networkpolicies" }
const remove: Change = { op: "delete", resource: policies, name: "zeabur-isolate-api" }

const isolate = {
  apiVersion: "networking.k8s.io/v1",
  kind: "NetworkPolicy",
  metadata: { name: "zeabur-isolate-api" },
  spec: { podSelector: { matchLabels: { app: "api" } }, policyTypes: ["Ingress"] },
} as unknown as Record<string, never>

describe("undoOf", () => {
  test("scaling is undone by scaling back to the pods there were", () => {
    const known = pods({ api: { desired: 2, ready: 2 } })
    expect(undoOf([{ op: "scale", workload: api, replicas: 4 }], known)).toStrictEqual([
      { op: "scale", workload: api, replicas: 2 },
    ])
  })

  test("a creation is undone by deleting what was created, at the version it was created at", () => {
    const applied = [
      {
        target: {
          apiVersion: "networking.k8s.io/v1",
          kind: "NetworkPolicy",
          plural: "networkpolicies",
          name: "zeabur-isolate-api",
        },
        resourceVersion: "812",
      },
    ]
    expect(undoOf([{ op: "create", object: isolate }], pods({}), applied)).toStrictEqual([
      { ...remove, resourceVersion: "812" },
    ])
    // Without the version the creation returned there is nothing safe to delete.
    expect(undoOf([{ op: "create", object: isolate }], pods({}))).toBe(undefined)
    expect(
      undoOf([{ op: "create", object: isolate }], pods({}), [
        { ...applied[0], resourceVersion: undefined },
      ]),
    ).toBe(undefined)
  })

  test("nothing else is, nor a scale without a known count", () => {
    expect(undoOf([remove], pods({}))).toBe(undefined)
    expect(undoOf([{ op: "restart", workload: api }], pods({}))).toBe(undefined)
    expect(undoOf([{ op: "scale", workload: api, replicas: 4 }], pods({}))).toBe(undefined)
    expect(
      undoOf(
        [
          { op: "scale", workload: api, replicas: 4 },
          { op: "resources", workload: api },
        ],
        pods({ api: { desired: 2, ready: 2 } }),
      ),
    ).toBe(undefined)
  })
})
