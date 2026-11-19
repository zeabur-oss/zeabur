import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { destinationOf } from "@/model/destination"

const api = { kind: "Deployment", name: "api" } as const

const isolate = {
  apiVersion: "networking.k8s.io/v1",
  kind: "NetworkPolicy",
  metadata: { name: "zeabur-isolate-api" },
  spec: { podSelector: { matchLabels: { app: "api" } }, policyTypes: ["Ingress"] },
} as unknown as Record<string, never>

describe("destinationOf", () => {
  const service = "/projects/shop/services/Deployment/api"
  test("a workload's change goes to its tab", () => {
    expect(destinationOf("shop", [{ op: "restart", workload: api }], undefined)).toBe(
      `${service}?tab=deployments`,
    )
    expect(destinationOf("shop", [{ op: "env", workload: api, changes: [] }], undefined)).toBe(
      `${service}?tab=variables`,
    )
    expect(destinationOf("shop", [{ op: "scale", workload: api, replicas: 2 }], undefined)).toBe(
      `${service}?tab=settings#compute`,
    )
  })

  test("an object's change goes to the object", () => {
    const plan = {
      objects: [
        {
          target: {
            apiVersion: "v1",
            kind: "ConfigMap",
            plural: "configmaps",
            name: "shop-config",
          },
        },
      ],
    } as unknown as Pick<Schemas.PlanResponse, "objects">
    expect(
      destinationOf(
        "shop",
        [{ op: "data", kind: "ConfigMap", name: "shop-config", changes: [] }],
        plan,
      ),
    ).toBe("/projects/shop/objects?object=core%2Fv1%2Fconfigmaps%2Fshop-config")
  })

  test("a policy goes to the networking tab of the service it was asked from", () => {
    expect(destinationOf("shop", [{ op: "create", object: isolate }], undefined, api)).toBe(
      `${service}?tab=networking`,
    )
  })

  test.each(["deleteProject", "forceDeleteProject"] as const)(
    "%s leads to the projects list",
    (op) => {
      expect(destinationOf("shop", [{ op }], undefined)).toBe("/projects")
    },
  )
})
