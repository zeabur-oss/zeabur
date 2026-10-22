import { describe, expect, test } from "bun:test"

import {
  building,
  builtContainers,
  durationOf,
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
