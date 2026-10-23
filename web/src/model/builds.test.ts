import { describe, expect, test } from "bun:test"

import {
  buildDeletions,
  building,
  builtContainers,
  durationOf,
  refusedCredentials,
  repositoryOf,
  runsLatest,
  type Build,
  type BuildRun,
  type Builds,
} from "@/model/builds"
import type { Service } from "@/model/service"

const digest = (n: string) => `sha256:${n.repeat(64)}`

const build = (over: Partial<Build> = {}): Build => ({
  name: "web",
  repository: "zot.zot.svc:5000/shop/web",
  latestImage: `zot.zot.svc:5000/shop/web:main-2-bbbbbbb@${digest("b")}`,
  suspended: false,
  keelWebhooks: [],
  ...over,
})

const service = (images: string[]) =>
  ({
    kind: "Deployment",
    name: "web",
    containers: images.map((image, i) => ({ name: `c${i}`, image })),
  }) as unknown as Service

describe("repositoryOf", () => {
  test("drops the tag and digest but keeps a registry's port", () => {
    expect(repositoryOf(`zot.zot.svc:5000/shop/web:main-1@${digest("a")}`)).toBe(
      "zot.zot.svc:5000/shop/web",
    )
    expect(repositoryOf("localhost:5000/web")).toBe("localhost:5000/web")
  })

  test("spells out Docker Hub", () => {
    expect(repositoryOf("nginx:1.27")).toBe("docker.io/library/nginx")
    expect(repositoryOf("acme/web")).toBe("docker.io/acme/web")
    expect(repositoryOf("index.docker.io/acme/web:1")).toBe("docker.io/acme/web")
    expect(repositoryOf("Docker.io/library/nginx")).toBe("docker.io/library/nginx")
  })
})

describe("builtContainers", () => {
  test("matches containers whose image is in a Build's repository, whatever the tag", () => {
    const found = builtContainers(
      service(["zot.zot.svc:5000/shop/web:main-1-aaaaaaa", "redis:7"]),
      [build()],
    )
    expect(found.map((b) => b.container)).toEqual(["c0"])
  })

  test("a Build with no repository yet matches nothing", () => {
    expect(
      builtContainers(service(["zot.zot.svc:5000/shop/web:1"]), [build({ repository: null })]),
    ).toEqual([])
  })
})

describe("runsLatest", () => {
  test("compares digests when both have one, else tags", () => {
    const b = build()
    expect(runsLatest(`zot.zot.svc:5000/shop/web:main-2-bbbbbbb@${digest("b")}`, b)).toBe(true)
    expect(runsLatest(`zot.zot.svc:5000/shop/web:main-2-bbbbbbb@${digest("c")}`, b)).toBe(false)
    expect(runsLatest("zot.zot.svc:5000/shop/web:main-2-bbbbbbb", b)).toBe(true)
    expect(runsLatest("zot.zot.svc:5000/shop/web:main-1-aaaaaaa", b)).toBe(false)
    expect(runsLatest("zot.zot.svc:5000/shop/web", b)).toBe(false)
    expect(runsLatest("x", build({ latestImage: null }))).toBeUndefined()
  })
})

test("a run's duration runs to now until it finishes", () => {
  const run = {
    started: "2026-10-23T08:00:00Z",
    finished: null,
  } as unknown as BuildRun
  expect(durationOf(run, Date.parse("2026-10-23T08:01:30Z"))).toBe(90_000)
  expect(durationOf({ ...run, finished: "2026-10-23T08:00:20Z" }, 0)).toBe(20_000)
  expect(durationOf({ ...run, started: null }, 0)).toBeUndefined()
})

test("a request to build again counts as building until a run answers it", () => {
  const now = Date.parse("2026-10-23T08:01:00Z")
  const run = (over: Partial<BuildRun>) =>
    ({ build: "web", phase: "Succeeded", created: "2026-10-23T07:00:00Z", ...over }) as BuildRun
  const data = (runs: BuildRun[], asked?: string) =>
    ({ builds: [build({ rebuildRequested: asked })], runs, keel: [] }) as unknown as Builds
  expect(building(data([run({})]), now)).toBe(false)
  expect(building(data([run({ phase: null })]), now)).toBe(true)
  expect(building(data([run({ phase: "Building" })]), now)).toBe(true)
  expect(building(data([run({})], "2026-10-23T08:00:30Z"), now)).toBe(true)
  const answered = run({ created: "2026-10-23T08:00:40Z" })
  expect(building(data([answered], "2026-10-23T08:00:30Z"), now)).toBe(false)
  expect(building(data([run({})], "2026-10-23T07:30:00Z"), now)).toBe(false)
})

test("a new Build counts as building until it has a run or a problem", () => {
  const fresh = (over: Partial<Build>) =>
    ({
      builds: [build({ latestImage: null, problem: null, ...over })],
      runs: [],
      keel: [],
    }) as unknown as Builds
  expect(building(fresh({}), 0)).toBe(true)
  const problem = { condition: "SourceReady", lasting: true } as Build["problem"]
  expect(building(fresh({ problem }), 0)).toBe(false)
})

describe("buildDeletions", () => {
  const image = "zot.zot.svc:5000/shop/web:pending"
  const named = (name: string, images: string[]) => ({ ...service(images), name }) as Service
  const web = named("web", [image])

  test("deletes the service's Build and the Secret the drawer made for it", () => {
    const deletions = buildDeletions(web, [web], [build({ credentials: "web-git" })])
    expect(deletions).toEqual([
      {
        op: "delete",
        resource: { group: "builder.zeabur.com", version: "v1alpha1", resource: "builds" },
        name: "web",
      },
      { op: "delete", resource: { version: "v1", resource: "secrets" }, name: "web-git" },
    ])
  })

  test("keeps a Build another service runs, and credentials Zeabur did not make or another Build reads", () => {
    const copy = named("copy", [image])
    expect(buildDeletions(web, [web, copy], [build({ credentials: "web-git" })])).toEqual([])
    expect(buildDeletions(web, [web], [build({ credentials: "github" })])).toHaveLength(1)
    const api = build({
      name: "api",
      repository: "zot.zot.svc:5000/shop/api",
      credentials: "web-git",
    })
    expect(buildDeletions(web, [web], [build({ credentials: "web-git" }), api])).toHaveLength(1)
    expect(buildDeletions(named("redis", ["redis:7"]), [web], [build()])).toEqual([])
  })
})

describe("refusedCredentials", () => {
  const refused = { condition: "SourceReady", reason: "AuthFailed", lasting: true }
  test("a deploy key over SSH, a token over HTTPS, nothing without credentials", () => {
    const ssh = build({ gitUrl: "git@github.com:acme/web.git", credentials: "web-git" })
    expect(refusedCredentials({ ...ssh, problem: refused }, undefined)).toBe("deployKey")
    const https = build({ gitUrl: "https://github.com/acme/web.git", credentials: "web-git" })
    expect(refusedCredentials({ ...https, problem: refused }, undefined)).toBe("token")
    expect(refusedCredentials({ ...ssh, credentials: null, problem: refused }, undefined)).toBe(
      undefined,
    )
    expect(
      refusedCredentials({ ...ssh, problem: { ...refused, reason: "RefNotFound" } }, undefined),
    ).toBe(undefined)
  })
})
