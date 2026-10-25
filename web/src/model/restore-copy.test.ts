import { describe, expect, test } from "bun:test"

import type { Target } from "@/model/connect"
import {
  ownSecrets,
  restoreChoices,
  restoreProblem,
  rewriteChanges,
  rewritten,
} from "@/model/restore-copy"

const literal = (name: string, value: string) => ({
  name,
  value,
  source: { kind: "Literal" as const },
})

describe("a project restored under a new name", () => {
  test("moves each in-cluster address of the original to the copy, short or full", () => {
    expect(rewritten("http://api.blog.svc:8080", "blog", "blog-restored-1025")).toBe(
      "http://api.blog.svc:8080".replace(".blog.svc", ".blog-restored-1025.svc"),
    )
    expect(rewritten("db.blog.svc.cluster.local", "blog", "copy")).toBe("db.copy.svc.cluster.local")
    expect(rewritten("https://blog.example.com", "blog", "copy")).toBeUndefined()
  })

  test("rewrites variables, ConfigMaps and Secrets that name the original, and nothing else", () => {
    const targets = [
      {
        kind: "Deployment",
        name: "web",
        containers: [
          {
            name: "web",
            env: [
              literal("API", "http://api.blog.svc:8080"),
              literal("SITE", "https://blog.example.com"),
              { name: "DB", source: { kind: "Secret", name: "db", key: "url" } },
            ],
          },
        ],
      },
    ] as unknown as Target[]
    const changes = rewriteChanges(
      "blog",
      "copy",
      targets,
      [{ name: "nginx", data: { upstream: "api.blog.svc.cluster.local:80", other: "x" } }],
      [{ name: "db", data: { url: "postgres://u:p@pg.blog.svc:5432/app" } }],
    )
    expect(changes).toEqual([
      {
        op: "env",
        workload: { kind: "Deployment", name: "web" },
        changes: [{ op: "set", name: "API", value: "http://api.copy.svc:8080" }],
      },
      {
        op: "data",
        kind: "ConfigMap",
        name: "nginx",
        changes: [{ op: "set", key: "upstream", value: "api.copy.svc.cluster.local:80" }],
      },
      {
        op: "data",
        kind: "Secret",
        name: "db",
        changes: [{ op: "set", key: "url", value: "postgres://u:p@pg.copy.svc:5432/app" }],
      },
    ])
  })

  test("reads no Secret a controller owns or Helm keeps its release in", () => {
    expect(
      ownSecrets([
        { name: "db" },
        { name: "sh.helm.release.v1.blog.v1" },
        { name: "pg-app", ownerKind: "Cluster" },
        { name: "api", ownerKind: null },
      ]),
    ).toEqual(["api", "db"])
  })
})

describe("choosing what to restore", () => {
  const projects = [
    { name: "shop", adopted: true },
    { name: "blog", adopted: true },
    { name: "kube-system", adopted: true, system: true },
    { name: "loose" },
  ]

  test("a backup of named projects offers those, even ones that are gone", () => {
    expect(restoreChoices(["wiki", "blog"], projects)).toEqual(["blog", "wiki"])
  })

  test("a backup of the whole cluster offers the projects Zeabur manages", () => {
    expect(restoreChoices([], projects)).toEqual(["blog", "shop"])
  })

  test("a project that still exists needs a new name nobody uses", () => {
    expect(restoreProblem("", "", projects)).toBe("restoreNone")
    expect(restoreProblem("blog", "blog", projects)).toBe("intoTaken")
    expect(restoreProblem("blog", "shop", projects)).toBe("intoTaken")
    expect(restoreProblem("blog", "", projects)).toBe("intoTaken")
    expect(restoreProblem("blog", "blog-restored", projects)).toBeUndefined()
  })

  test("a project that is gone restores under its own name", () => {
    expect(restoreProblem("wiki", "", projects)).toBeUndefined()
  })
})
