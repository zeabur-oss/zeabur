import { describe, expect, test } from "bun:test"

import {
  KEEL_WEBHOOK,
  branchOf,
  buildProblem,
  builtContainers,
  repositoryOf,
  type Build,
  type BuildRun,
} from "@/model/builds"
import {
  branchProblem,
  buildOptionsChanges,
  buildOptionsOf,
  createGitService,
  deployKeyHost,
  deployKeyPage,
  gitMissing,
  gitObjects,
  newGitService,
  parseRepository,
  pathProblem,
  tokenUser,
  urlProblem,
  type GitService,
} from "@/model/git-service"
import { reasonWord } from "@/model/service"

const form = (patch: Partial<GitService>): GitService => ({
  ...newGitService(),
  url: "https://github.com/acme/site",
  name: "site",
  port: "3000",
  ...patch,
})

type Object = Record<string, any>
const find = (objects: Object[], kind: string) => objects.find((o) => o.kind === kind) as Object

describe("repository addresses", () => {
  test("HTTPS, scp-like and ssh:// addresses name the same repository", () => {
    const want = { host: "github.com", path: "acme/site" }
    expect(parseRepository("https://github.com/acme/site")).toEqual(want)
    expect(parseRepository("https://github.com/acme/site.git/")).toEqual(want)
    expect(parseRepository("git@github.com:acme/site.git")).toEqual(want)
    expect(parseRepository("ssh://git@github.com/acme/site.git")).toEqual(want)
    expect(parseRepository("https://gitlab.com/group/sub/app")).toEqual({
      host: "gitlab.com",
      path: "group/sub/app",
    })
  })

  test("anything else, or a password in the address, is refused", () => {
    expect(urlProblem("")).toBe("empty")
    expect(urlProblem("github.com/acme/site")).toBe("invalid")
    expect(urlProblem("https://github.com/acme")).toBe("invalid")
    expect(urlProblem("ftp://example.com/a/b")).toBe("invalid")
    expect(urlProblem("https://me:ghp_secret@github.com/acme/site")).toBe("invalid")
  })

  test("deploy keys are for GitHub and GitLab, each with its own page", () => {
    const github = parseRepository("https://github.com/acme/site")
    const gitea = parseRepository("https://git.example.com/acme/site")
    expect(deployKeyHost(github)).toBe("github.com")
    expect(deployKeyHost(gitea)).toBeUndefined()
    expect(deployKeyPage(github!)).toBe("https://github.com/acme/site/settings/keys/new")
    expect(deployKeyPage(parseRepository("git@gitlab.com:g/app.git")!)).toBe(
      "https://gitlab.com/g/app/-/settings/repository#js-deploy-keys-settings",
    )
    expect(tokenUser("github.com")).toBe("x-access-token")
    expect(tokenUser("gitlab.com")).toBe("oauth2")
    expect(tokenUser("git.example.com")).toBe("")
  })

  test("branches and folders are checked as Git takes them", () => {
    expect(branchProblem("main")).toBeUndefined()
    expect(branchProblem("release/1.2")).toBeUndefined()
    expect(branchProblem("")).toBeUndefined()
    expect(branchProblem("a..b")).toBe("invalid")
    expect(pathProblem("")).toBeUndefined()
    expect(pathProblem("./apps/web/")).toBeUndefined()
    expect(pathProblem("docker/Dockerfile.prod")).toBeUndefined()
    expect(pathProblem("../etc")).toBe("invalid")
    expect(pathProblem("apps/.cache")).toBe("invalid")
  })
})

describe("the objects", () => {
  test("a public repository: a Build that tells Keel, and a Deployment Keel moves", () => {
    const objects = gitObjects(form({}), "shop", "zot.zot.svc:5000", {})
    expect(objects.map((o) => o.kind)).toEqual(["Secret", "Deployment", "Service", "Build"])
    const build = find(objects, "Build")
    expect(build.apiVersion).toBe("builder.zeabur.com/v1alpha1")
    expect(build.spec).toEqual({
      builderRef: "railpack",
      source: { git: { url: "https://github.com/acme/site.git", ref: "HEAD" } },
      output: { image: "zot.zot.svc:5000/shop/site" },
      notify: [{ type: "keel", url: KEEL_WEBHOOK }],
    })
    const deployment = find(objects, "Deployment")
    expect(deployment.metadata.annotations).toEqual({ "keel.sh/policy": "force" })
    const image = deployment.spec.template.spec.containers[0].image
    expect(image).toBe("zot.zot.svc:5000/shop/site:pending")
    expect(repositoryOf(image)).toBe(repositoryOf(build.spec.output.image))
    expect(find(objects, "Secret").stringData).toEqual({ PORT: "3000" })
  })

  test("the Builds tab finds the service's Build by repository, as for any built image", () => {
    const service = {
      containers: [{ name: "site", image: "zot.zot.svc:5000/shop/site:pending" }],
    } as Parameters<typeof builtContainers>[0]
    const build = { name: "site", repository: "zot.zot.svc:5000/shop/site" } as Parameters<
      typeof builtContainers
    >[1][number]
    expect(builtContainers(service, [build]).map((b) => b.build.name)).toEqual(["site"])
  })

  test("a token goes in a basic-auth Secret the Build names; the root directory is the context", () => {
    const objects = gitObjects(
      form({
        access: "token",
        username: "x-access-token",
        token: " ghp_1 ",
        directory: "./apps/web/",
        method: "dockerfile",
        port: "",
      }),
      "shop",
      "zot:5000",
      {},
    )
    const secret = objects.find((o) => (o.metadata as Object).name === "site-git") as Object
    expect(secret.type).toBe("kubernetes.io/basic-auth")
    expect(secret.metadata.labels["builder.zeabur.com/git-credentials"]).toBe("true")
    expect(secret.stringData).toEqual({ username: "x-access-token", password: "ghp_1" })
    const build = find(objects, "Build")
    expect(build.spec.builderRef).toBe("dockerfile")
    expect(build.spec.source.git.contextDir).toBe("apps/web")
    expect(build.spec.source.credentialsRef).toEqual({ name: "site-git" })
    expect(find(objects, "Service")).toBeUndefined()
  })

  test("build variables go to their own Secret, which the Build reads key by key", () => {
    const objects = gitObjects(
      form({
        method: "dockerfile",
        dockerfile: "./docker/Dockerfile.prod",
        buildVariables: [{ name: "NPM_TOKEN", value: "npm_1" }],
      }),
      "shop",
      "zot:5000",
      {},
    )
    const secret = objects.find((o) => (o.metadata as Object).name === "site-build-env") as Object
    expect(secret.stringData).toEqual({ NPM_TOKEN: "npm_1" })
    const build = find(objects, "Build")
    expect(build.spec.source.git.dockerfile).toBe("docker/Dockerfile.prod")
    expect(build.spec.env).toEqual([
      {
        name: "NPM_TOKEN",
        valueFrom: { secretKeyRef: { name: "site-build-env", key: "NPM_TOKEN" } },
      },
    ])
    expect(JSON.stringify(build)).not.toContain("npm_1")
    const railpack = find(gitObjects(form({ dockerfile: "x" }), "shop", "zot:5000", {}), "Build")
    expect(railpack.spec.source.git.dockerfile).toBeUndefined()
    expect(railpack.spec.env).toBeUndefined()
  })

  test("a deploy key is a change of its own, and the Build uses SSH", () => {
    const changes = createGitService(
      form({ access: "deployKey", sealedKey: "k1.sealed" }),
      "shop",
      "zot:5000",
      {},
    )
    expect(changes.at(-1)).toEqual({ op: "deployKey", name: "site-git", key: "k1.sealed" })
    const build = changes.find(
      (c) => c.op === "create" && (c.object as Object).kind === "Build",
    ) as { object: Object }
    expect(build.object.spec.source.git.url).toBe("git@github.com:acme/site.git")
    expect(JSON.stringify(changes)).not.toContain("PRIVATE KEY")
  })

  test("a repository picked from the GitHub App names its installation and no Secret", () => {
    const objects = gitObjects(
      form({ access: "githubApp", installationId: 4242, branch: "trunk" }),
      "shop",
      "zot:5000",
      {},
    )
    const build = find(objects, "Build")
    expect(build.spec.source.git.url).toBe("https://github.com/acme/site.git")
    expect(build.spec.source.git.ref).toBe("trunk")
    expect(build.spec.source.githubApp).toEqual({ installationId: 4242 })
    expect(build.spec.source.credentialsRef).toBeUndefined()
    expect(objects.some((o) => (o.metadata as Object).name === "site-git")).toBe(false)
  })

  test("no branch follows the default branch; a branch is taken as typed", () => {
    expect(newGitService().branch).toBe("")
    const ref = (branch: string) =>
      find(gitObjects(form({ branch }), "shop", "zot:5000", {}), "Build").spec.source.git.ref
    expect(ref("  ")).toBe("HEAD")
    expect(ref(" release/1.2 ")).toBe("release/1.2")
  })

  test("a new Git service is Strict and runs as the user its image names", () => {
    expect(newGitService().security).toBe("restricted")
    const pod = find(gitObjects(form({}), "shop", "zot:5000", {}), "Deployment").spec.template.spec
    expect(pod.automountServiceAccountToken).toBe(false)
    expect(pod.securityContext).toEqual({
      runAsNonRoot: true,
      seccompProfile: { type: "RuntimeDefault" },
    })
    expect(pod.containers[0].securityContext).toEqual({
      allowPrivilegeEscalation: false,
      capabilities: { drop: ["ALL"] },
    })
    const changes = createGitService(form({}), "shop", "zot:5000", {}, "restricted")
    expect(changes.some((c) => c.op === "podSecurity")).toBe(false)
  })

  test("a Compatible Git service lowers a Strict project in the same plan", () => {
    const changes = createGitService(
      form({ security: "baseline" }),
      "shop",
      "zot:5000",
      {},
      "restricted",
    )
    expect(changes.at(-1)).toEqual({ op: "podSecurity", level: "baseline" })
    const deployment = changes.find(
      (c) => c.op === "create" && (c.object as Object).kind === "Deployment",
    ) as { object: Object }
    expect(deployment.object.spec.template.spec.securityContext).toEqual({
      seccompProfile: { type: "RuntimeDefault" },
    })
  })

  test("a public domain adds an Ingress", () => {
    const objects = gitObjects(
      form({ publicUrl: true, host: "site.example.com" }),
      "shop",
      "zot:5000",
      { clusterIssuer: "letsencrypt" },
    )
    expect(find(objects, "Ingress").spec.rules[0].host).toBe("site.example.com")
  })
})

describe("build options in the service's settings", () => {
  const built = (env: Build["env"]) =>
    ({ name: "site", contextDir: "apps/web", dockerfile: null, env }) as unknown as Build
  const wired = { name: "NPM_TOKEN", secret: "site-build-env", key: "NPM_TOKEN" }

  test("nothing changed plans nothing", () => {
    const build = built([wired])
    expect(buildOptionsChanges(build, buildOptionsOf(build))).toEqual([])
  })

  test("a new value of a wired variable changes only its Secret", () => {
    const build = built([wired])
    expect(
      buildOptionsChanges(build, {
        ...buildOptionsOf(build),
        set: [{ name: "NPM_TOKEN", value: "npm_2" }],
      }),
    ).toEqual([
      {
        op: "data",
        kind: "Secret",
        name: "site-build-env",
        changes: [{ op: "set", key: "NPM_TOKEN", value: "npm_2" }],
      },
    ])
  })

  test("the first variable makes the Secret; a removed one leaves the Build and its key", () => {
    const first = buildOptionsChanges(built([]), {
      directory: "",
      dockerfile: "Dockerfile.prod",
      removed: [],
      set: [{ name: "SENTRY_TOKEN", value: "s" }],
    })
    expect(first[0]).toEqual({
      op: "buildOptions",
      build: "site",
      contextDir: "",
      dockerfile: "Dockerfile.prod",
      env: [
        {
          op: "set",
          name: "SENTRY_TOKEN",
          valueFrom: { secretName: "site-build-env", key: "SENTRY_TOKEN" },
        },
      ],
    })
    expect(first[1]).toMatchObject({ op: "create", object: { kind: "Secret" } })
    const literal = { name: "NODE_ENV", secret: null, key: null }
    const build = built([wired, literal])
    expect(
      buildOptionsChanges(build, { ...buildOptionsOf(build), removed: ["NPM_TOKEN", "NODE_ENV"] }),
    ).toEqual([
      {
        op: "buildOptions",
        build: "site",
        env: [
          { op: "remove", name: "NPM_TOKEN" },
          { op: "remove", name: "NODE_ENV" },
        ],
      },
      {
        op: "data",
        kind: "Secret",
        name: "site-build-env",
        changes: [{ op: "remove", key: "NPM_TOKEN" }],
      },
    ])
  })
})

describe("what the cluster lacks", () => {
  const capability = (name: string, existing: boolean) =>
    ({ name, existing, components: [], evidence: [], installations: [] }) as never
  const noZot = capability("container-registry", false)
  const noKeel = capability("image-updates", false)
  test.each([
    ["the Builder", "not-installed", [], "zot", "builder"],
    ["a registry to install", "installed", [noZot, noKeel], "", "registry"],
    ["the registry setting", "installed", undefined, "", "registrySetting"],
    [
      "Keel, past a configured registry detection doesn't know",
      "installed",
      [noZot, noKeel],
      "ghcr.io/acme",
      "keel",
    ],
    ["nothing with a configured registry", "installed", [noZot], "harbor.example.com", undefined],
    ["nothing while unknown", undefined, undefined, undefined, undefined],
  ] as const)("%s", (_, builds, capabilities, registry, missing) => {
    expect(gitMissing({ builds, capabilities: capabilities as never, registry })).toBe(missing)
  })
})

test("a failed first build reads as one", () => {
  expect(reasonWord("BuildFailed", false)).toBe("buildFailed")
})

describe("the Builds tab", () => {
  const build = (patch: Partial<Build>) =>
    ({ name: "site", gitRef: "HEAD", resolvedRef: null, problem: null, ...patch }) as Build
  test("shows the branch the default branch resolved to, or none before it resolves", () => {
    expect(branchOf(build({ resolvedRef: "refs/heads/master" }))).toBe("master")
    expect(branchOf(build({}))).toBeUndefined()
    expect(branchOf(build({ gitRef: null }))).toBeUndefined()
    expect(branchOf(build({ gitRef: "release/1.2" }))).toBe("release/1.2")
    expect(branchOf(build({ gitRef: "main", resolvedRef: "refs/heads/main" }))).toBe("main")
  })

  test("a source problem counts unless a run is under way", () => {
    const problem = {
      condition: "SourceReady",
      reason: "RefNotFound",
      message: "no branch or tag `main`",
      since: null,
      lasting: true,
    }
    const failed = build({ problem })
    const run = (phase: string) => ({ name: "site-1", build: "site", phase }) as BuildRun
    expect(buildProblem(failed, undefined)).toEqual(problem)
    expect(buildProblem(failed, run("Failed"))).toEqual(problem)
    expect(buildProblem(failed, run("Building"))).toBeUndefined()
    expect(buildProblem(build({}), undefined)).toBeUndefined()
  })
})
