import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { check, documentsOf, inCreateOrder, lineOf } from "@/model/create"

const deployment = `apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
spec:
  template:
    spec:
      containers:
        - name: api
          image: example/api:1
`
const service = `apiVersion: v1
kind: Service
metadata:
  name: "api"
  namespace: prod
spec:
  ports: [{port: 80}]
`

const creatable: Schemas["CreatableList"] = {
  items: [
    {
      group: "apps",
      version: "v1",
      resource: "deployments",
      kind: "Deployment",
      custom: false,
      access: "allowed",
    },
    {
      group: "",
      version: "v1",
      resource: "services",
      kind: "Service",
      custom: false,
      access: "allowed",
    },
    {
      group: "",
      version: "v1",
      resource: "secrets",
      kind: "Secret",
      custom: false,
      access: "denied",
    },
  ],
  clusterScoped: [{ group: "rbac.authorization.k8s.io", kind: "ClusterRole" }],
}

describe("documentsOf", () => {
  test("splits documents and names each field's line in the whole text", () => {
    const docs = documentsOf(`# objects\n---\n${deployment}---\n\n---\n${service}`)
    expect(docs.map((d) => [d.index, d.kind, d.name, d.line])).toEqual([
      [0, "Deployment", "api", 3],
      [1, "Service", "api", 16],
    ])
    expect(docs[1]?.namespace).toBe("prod")
    expect(lineOf(docs[0]!, "spec.template.spec.containers[0].image")).toBe(12)
    expect(lineOf(docs[1]!, "spec.ports[0].port")).toBe(22)
    expect(lineOf(docs[1]!, "metadata.labels[app]")).toBe(18)
    expect(lineOf(docs[1]!, "nothing.here")).toBe(16)
  })

  test("a document that does not read names its line", () => {
    const [doc] = documentsOf("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n   data: x\n")
    expect(doc?.error).toBeGreaterThanOrEqual(4)
  })
})

test("creation order follows what workloads need", () => {
  const docs = documentsOf(
    `${deployment}---\n${service}---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: s\n`,
  )
  expect(inCreateOrder(docs).map((d) => d.kind)).toEqual(["Secret", "Service", "Deployment"])
})

describe("check", () => {
  test("finds what the server would refuse, at its line", () => {
    const text = [
      service,
      "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nmetadata:\n  name: r\n",
      "apiVersion: traefik.io/v1alpha1\nkind: Middleware\nmetadata:\n  name: m\n",
      "apiVersion: v1\nkind: Secret\nmetadata:\n  name: s\n",
      "apiVersion: v1\nkind: ConfigMap\nmetadata: {}\n",
      deployment,
    ].join("---\n")
    const docs = documentsOf(text)
    const found = check(docs, "shop", {
      creatable,
      existing: new Set(["apps/deployments/api"]),
    })
    expect(found.map((f) => [f.code, f.document, f.line])).toEqual([
      ["otherNamespace", 0, 5],
      ["clusterScoped", 1, 10],
      ["notServed", 2, 15],
      ["rights", 3, 20],
      ["missingField", 4, 26],
      ["exists", 5, 31],
    ])
  })

  test("the same object twice is found at the second", () => {
    const docs = documentsOf(`${deployment}---\n${deployment}`)
    expect(check(docs, "shop", { creatable }).map((f) => [f.code, f.document])).toEqual([
      ["duplicate", 1],
    ])
  })
})
