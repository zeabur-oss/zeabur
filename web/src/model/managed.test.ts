import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { reapplierOf, toolNotes } from "@/model/managed"

const object = (name: string, owners: Schemas["PlanFieldOwners"][]) =>
  ({ target: { kind: "Deployment", name }, owners }) as unknown as Schemas["PlannedObject"]

describe("toolNotes", () => {
  test("names the services a plan left out, by the tool that manages them", () => {
    const notes = toolNotes({ objects: [] }, [
      { name: "demo-gateway", tool: "Helm" },
      { name: "web", tool: "Argo CD" },
      { name: "api", tool: "Helm" },
    ])
    expect(notes).toEqual([
      { kind: "skipped", tool: "Helm", names: ["demo-gateway", "api"] },
      { kind: "skipped", tool: "Argo CD", names: ["web"] },
    ])
  })

  test("names the fields a tool will put back, and nothing for Zeabur's own", () => {
    const notes = toolNotes({
      objects: [
        object("api", [
          { path: "spec.replicas", managers: ["kubectl-client-side-apply"] },
          { path: "spec.template.spec.containers[api].image", managers: ["zeabur"] },
        ]),
        object("worker", [{ path: "spec.replicas", managers: ["kube-controller-manager"] }]),
      ],
    })
    expect(notes).toEqual([
      { kind: "putBack", tool: "kubectl", object: "api", paths: ["spec.replicas"] },
    ])
  })

  test("a plan nothing else owns has no notes", () => {
    expect(toolNotes({ objects: [object("api", [])] })).toEqual([])
  })
})

test("managers are read as the tools that write again", () => {
  expect(reapplierOf("helm")).toBe("Helm")
  expect(reapplierOf("argocd-controller")).toBe("Argo CD")
  expect(reapplierOf("kustomize-controller")).toBe("Flux")
  expect(reapplierOf("kubectl-client-side-apply")).toBe("kubectl")
  expect(reapplierOf("kubectl-edit")).toBeUndefined()
  expect(reapplierOf("zeabur")).toBeUndefined()
})
