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
  createGitService,
  deployKeyHost,
  deployKeyPage,
  directoryProblem,
  gitMissing,
  gitObjects,
  newGitService,
  parseRepository,
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
    expect(directoryProblem("")).toBeUndefined()
    expect(directoryProblem("./apps/web/")).toBeUndefined()
    expect(directoryProblem("../etc")).toBe("invalid")
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

  test("no branch follows the default branch; a branch is taken as typed", () => {
    expect(newGitService().branch).toBe("")
    const ref = (branch: string) =>
      find(gitObjects(form({ branch }), "shop", "zot:5000", {}), "Build").spec.source.git.ref
    expect(ref("  ")).toBe("HEAD")
    expect(ref(" release/1.2 ")).toBe("release/1.2")
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

describe("what the cluster lacks", () => {
  const capability = (name: string, existing: boolean) =>
    ({ name, existing, components: [], evidence: [], installations: [] }) as never
  test("the Builder, the registry, Keel, then the registry setting; unknown stays open", () => {
    expect(gitMissing({ builds: "not-installed", capabilities: [], registry: "zot" })).toBe(
      "builder",
    )
    expect(
      gitMissing({
        builds: "installed",
        capabilities: [capability("container-registry", false), capability("image-updates", false)],
        registry: "zot",
      }),
    ).toBe("registry")
    expect(
      gitMissing({
        builds: "installed",
        capabilities: [capability("container-registry", true), capability("image-updates", false)],
        registry: "zot",
      }),
    ).toBe("keel")
    expect(gitMissing({ builds: "installed", capabilities: undefined, registry: "" })).toBe(
      "registrySetting",
    )
    expect(
      gitMissing({ builds: undefined, capabilities: undefined, registry: undefined }),
    ).toBeUndefined()
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
