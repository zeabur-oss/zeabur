import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"

import {
  buildDeletions,
  builtContainers,
  durationOf,
  failureText,
  reasonKey,
  repositoryOf,
  runsLatest,
  shortSha,
  takesNewBuilds,
  type Build,
  type BuildRun,
} from "@/model/builds"
import type { Service } from "@/model/service"

const digest = (n: string) => `sha256:${n.repeat(64)}`

const build = (over: Partial<Build> = {}): Build => ({
  name: "web",
  repository: "zot.zot.svc:5000/shop/web",
  latestImage: `zot.zot.svc:5000/shop/web:main-2-bbbbbbb@${digest("b")}`,
  suspended: false,
  archive: false,
  awaitingUpload: false,
  keelWebhooks: [],
  env: [],
  outOfMemory: false,
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
    startedAt: "2026-10-23T08:00:00Z",
    finishedAt: null,
  } as unknown as BuildRun
  expect(durationOf(run, Date.parse("2026-10-23T08:01:30Z"))).toBe(90_000)
  expect(durationOf({ ...run, finishedAt: "2026-10-23T08:00:20Z" }, 0)).toBe(20_000)
  expect(durationOf({ ...run, startedAt: null }, 0)).toBeUndefined()
})

describe("buildDeletions", () => {
  const image = "zot.zot.svc:5000/shop/web:pending"
  const named = (name: string, images: string[]) => ({ ...service(images), name }) as Service
  const web = named("web", [image])

  test("deletes the service's Build and the Secret the drawer made for it", () => {
    const deletions = buildDeletions(
      web,
      [web],
      [build({ credentials: { name: "web-git", own: true } })],
    )
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
    expect(
      buildDeletions(web, [web, copy], [build({ credentials: { name: "web-git", own: true } })]),
    ).toEqual([])
    const kinds = (deletions: ReturnType<typeof buildDeletions>) =>
      deletions.map((d) => (d.op === "delete" ? d.resource.resource : d.op))
    expect(
      kinds(buildDeletions(web, [web], [build({ credentials: { name: "github", own: false } })])),
    ).toEqual(["builds"])
    const api = build({
      name: "api",
      repository: "zot.zot.svc:5000/shop/api",
      credentials: { name: "web-git", own: false },
    })
    expect(
      kinds(
        buildDeletions(web, [web], [build({ credentials: { name: "web-git", own: true } }), api]),
      ),
    ).toEqual(["builds"])
    expect(buildDeletions(named("redis", ["redis:7"]), [web], [build()])).toEqual([])
  })
})

describe("shortSha", () => {
  test("a commit and an uploaded folder's digest both read as seven hexadecimal digits", () => {
    expect(shortSha("3f9a1c2b7d4e5f60718293a4b5c6d7e8f9012345")).toBe("3f9a1c2")
    expect(shortSha(digest("a"))).toBe("aaaaaaa")
    expect(shortSha(undefined)).toBeUndefined()
  })
})

describe("failureText", () => {
  test("the build's problem comes before the run's words, and those before the resource's", () => {
    const run = { message: "run said", reason: "RunReason" } as BuildRun
    expect(failureText(build({ message: "build said" }), run)).toBe("run said")
    expect(failureText(build({ reason: "BuildReason" }), undefined)).toBe("BuildReason")
    expect(failureText(undefined, undefined)).toBeUndefined()
  })
})

describe("reasonKey", () => {
  test("a reason the builder reports gets its own sentence", () => {
    expect(reasonKey("PlanFailed")).toBe("builds.reason.PlanFailed")
  })

  test("a reason Zeabur does not know shares one sentence", () => {
    expect(reasonKey("SomethingNew")).toBe("builds.reason.other")
    expect(reasonKey(undefined)).toBe("builds.reason.other")
  })
})

test("the build switch is on only when Keel takes every new build", () => {
  const updating = (setting: Schemas.UpdateSetting | null) =>
    ({ imageUpdates: { allowed: [], setting } }) as unknown as Service
  expect(takesNewBuilds(updating({ kind: "newBuilds" }))).toBe(true)
  expect(takesNewBuilds(updating({ kind: "other", policy: "force" }))).toBe(true)
  expect(takesNewBuilds(updating({ kind: "image", policy: "sameTag" }))).toBe(false)
  expect(takesNewBuilds(updating(null))).toBe(false)
})
