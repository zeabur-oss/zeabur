import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  branchOf,
  buildProblem,
  builtContainers,
  sourceContainer,
  type Build,
  type BuildRun,
} from "@/model/builds"
import {
  branchProblem,
  buildOptionsChanges,
  buildOptionsOf,
  buildProfile,
  defaultPort,
  derivedMethod,
  dockerfileAt,
  createGitService,
  deployKeyHost,
  deployKeyPage,
  keyscanCommand,
  pinnedHost,
  gitChange,
  gitNeeds,
  gitSourceChange,
  gitSourceOf,
  newGitService,
  newGitSource,
  offeredMethods,
  parseRepository,
  pathProblem,
  repositoryProblems,
  tokenUser,
  urlProblem,
  withPathsOf,
  type GitService,
} from "@/model/git-service"

/** The repository a change's Build reads. */
const source = (change: Schemas.BuiltServiceChange) =>
  change.build.source as Extract<Schemas.BuildSource, { type: "git" }>

const form = (patch: Partial<GitService>): GitService => ({
  ...newGitService(),
  url: "https://github.com/acme/site",
  name: "site",
  port: "3000",
  publicUrl: false,
  ...patch,
})

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

  test("a deploy key is for the SSH host, with its port, and pinned only for GitHub and GitLab", () => {
    expect(deployKeyHost("https://github.com/acme/site")).toBe("github.com")
    expect(deployKeyHost("https://git.example.com:8443/acme/site")).toBe("git.example.com")
    expect(deployKeyHost("ssh://git@git.example.com:2222/acme/site.git")).toBe(
      "git.example.com:2222",
    )
    expect(deployKeyHost("git@git.example.com:acme/site.git")).toBe("git.example.com")
    expect(deployKeyHost("not a url")).toBeUndefined()
    expect(pinnedHost("github.com")).toBe(true)
    expect(pinnedHost("git.example.com")).toBe(false)
    expect(keyscanCommand("git.example.com")).toBe("ssh-keyscan -t ed25519,ecdsa git.example.com")
    expect(keyscanCommand("git.example.com:2222")).toBe(
      "ssh-keyscan -t ed25519,ecdsa -p 2222 git.example.com",
    )
    expect(deployKeyPage(parseRepository("https://github.com/acme/site")!)).toBe(
      "https://github.com/acme/site/settings/keys/new",
    )
    expect(deployKeyPage(parseRepository("git@gitlab.com:g/app.git")!)).toBe(
      "https://gitlab.com/g/app/-/settings/repository#js-deploy-keys-settings",
    )
    expect(deployKeyPage(parseRepository("https://git.example.com/acme/site")!)).toBeUndefined()
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

describe("a Git source for a running service", () => {
  test("names the workload and reads the repository as a new Git service does", () => {
    const change = gitSourceChange(
      {
        ...newGitSource("dockerfile"),
        url: " git@github.com:acme/site.git ",
        branch: "main",
        directory: "./web/",
        access: "deployKey",
      },
      { kind: "Deployment", name: "site" },
    )
    expect(change).toEqual({
      op: "setBuild",
      workload: { kind: "Deployment", name: "site" },
      build: {
        source: {
          type: "git",
          url: "git@github.com:acme/site.git",
          branch: "main",
          access: { type: "deployKey" },
        },
        method: "dockerfile",
        directory: "web",
      },
    })
  })
})

describe("changing a Git service's source", () => {
  const build = (patch: Partial<Build>) =>
    ({
      name: "site",
      archive: false,
      builder: "railpack",
      gitUrl: "https://github.com/acme/site.git",
      gitRef: "HEAD",
      env: [],
      ...patch,
    }) as Build
  const workload = { kind: "Deployment", name: "site" } as const

  test("the form starts from the Build, and sent unchanged reads the same repository", () => {
    const current = build({
      builder: "dockerfile",
      gitRef: "release/1.2",
      contextDir: "apps/web",
      dockerfile: "docker/Dockerfile",
    })
    expect(gitSourceChange(gitSourceOf(current), workload, "site")).toEqual({
      op: "setBuild",
      workload,
      container: "site",
      build: {
        source: {
          type: "git",
          url: "https://github.com/acme/site.git",
          branch: "release/1.2",
          access: { type: "public" },
        },
        method: "dockerfile",
        directory: "apps/web",
        dockerfile: "docker/Dockerfile",
      },
    })
  })

  test("the paths sent are the Build's as they are now, not as the form found them", () => {
    const form = {
      ...gitSourceOf(build({ contextDir: "old" })),
      branch: "dev",
    }
    const now = build({
      builder: "dockerfile",
      contextDir: "apps/web",
      dockerfile: "Dockerfile.prod",
    })
    expect(
      gitSourceChange(withPathsOf({ ...form, method: "dockerfile" }, now), workload),
    ).toMatchObject({
      build: {
        source: { branch: "dev" },
        directory: "apps/web",
        dockerfile: "Dockerfile.prod",
      },
    })
    expect(withPathsOf(form, undefined)).toBe(form)
  })

  test("the default branch stays the default", () => {
    expect(gitSourceOf(build({ gitRef: "HEAD", resolvedRef: "refs/heads/main" })).branch).toBe("")
    expect(gitSourceOf(build({ gitRef: "refs/heads/dev" })).branch).toBe("dev")
  })

  test("an SSH address keeps its deploy key; HTTPS credentials take a token typed again", () => {
    const ssh = gitSourceOf(
      build({
        gitUrl: "git@github.com:acme/site.git",
        credentials: { name: "site-git", own: true },
      }),
    )
    expect(ssh.access).toBe("deployKey")
    expect(gitSourceChange(ssh, workload)).toMatchObject({
      build: {
        source: {
          url: "git@github.com:acme/site.git",
          access: { type: "deployKey" },
        },
      },
    })
    const token = gitSourceOf(build({ credentials: { name: "site-git", own: true } }))
    expect(token).toMatchObject({ access: "token", token: "" })
    expect(repositoryProblems(token, true).token).toBe(true)
  })

  test("an uploaded folder's Build starts an empty repository with its paths and method", () => {
    expect(
      gitSourceOf(
        build({
          archive: true,
          gitUrl: null,
          builder: "dockerfile",
          contextDir: "web",
        }),
      ),
    ).toMatchObject({
      url: "",
      method: "dockerfile",
      directory: "web",
      access: "public",
    })
  })

  test("the change goes to the container that runs the Build's image", () => {
    const service = {
      containers: [
        { name: "migrate", image: "zot:5000/shop/site:1", init: true },
        { name: "proxy", image: "nginx:1" },
        { name: "site", image: "zot:5000/shop/site:1" },
      ],
    } as Parameters<typeof sourceContainer>[0]
    expect(sourceContainer(service, build({ repository: "zot:5000/shop/site" }))).toBe("site")
    expect(sourceContainer(service, undefined)).toBe("proxy")
  })

  test("a Build that only an init container runs offers no source change", () => {
    const service = {
      containers: [
        { name: "migrate", image: "zot:5000/shop/migrate:1", init: true },
        { name: "proxy", image: "nginx:1" },
      ],
    } as Parameters<typeof sourceContainer>[0]
    expect(sourceContainer(service, build({ repository: "zot:5000/shop/migrate" }))).toBeUndefined()
  })
})

describe("the change", () => {
  test("a public repository builds from its default branch, with the port and no host", () => {
    expect(gitChange(form({ branch: "  " }))).toEqual({
      name: "site",
      port: 3000,
      variables: [],
      size: "small",
      security: "restricted",
      build: {
        source: {
          type: "git",
          url: "https://github.com/acme/site",
          access: { type: "public" },
        },
        method: "railpack",
        buildVariables: [],
      },
    })
    expect(source(gitChange(form({ branch: " release/1.2 " }))).branch).toBe("release/1.2")
  })

  test("the Builds tab finds the service's Build by repository, as for any built image", () => {
    const service = {
      containers: [{ name: "site", image: "zot.zot.svc:5000/shop/site:pending" }],
    } as Parameters<typeof builtContainers>[0]
    const build = {
      name: "site",
      repository: "zot.zot.svc:5000/shop/site",
    } as Parameters<typeof builtContainers>[1][number]
    expect(builtContainers(service, [build]).map((b) => b.build.name)).toEqual(["site"])
  })

  test("a token is trimmed; paths are cleaned; a Dockerfile only for a Dockerfile build", () => {
    const change = gitChange(
      form({
        access: "token",
        username: "x-access-token",
        token: " ghp_1 ",
        directory: "./apps/web/",
        method: "dockerfile",
        dockerfile: "./docker/Dockerfile.prod",
        port: "",
      }),
    )
    expect(source(change).access).toEqual({
      type: "token",
      username: "x-access-token",
      token: "ghp_1",
    })
    expect(change.build.directory).toBe("apps/web")
    expect(change.build.dockerfile).toBe("docker/Dockerfile.prod")
    expect(change.port).toBeUndefined()
    expect(gitChange(form({ dockerfile: "x" })).build.dockerfile).toBeUndefined()
  })

  test("a deploy key sends the host's lines only for a host Zeabur does not pin", () => {
    const lines = "[git.example.com]:2222 ssh-ed25519 AAAA"
    expect(source(gitChange(form({ access: "deployKey", knownHosts: lines }))).access).toEqual({
      type: "deployKey",
    })
    const other = form({
      url: "ssh://git@git.example.com:2222/acme/site.git",
      access: "deployKey",
      knownHosts: lines,
    })
    expect(source(gitChange(other)).access).toEqual({
      type: "deployKey",
      knownHosts: lines,
    })
  })

  test("a repository picked from the GitHub App names its installation", () => {
    expect(source(gitChange(form({ access: "githubApp", installationId: 4242 }))).access).toEqual({
      type: "githubApp",
      installationId: 4242,
    })
  })

  test("Railpack builds are Strict and Dockerfile builds Compatible", () => {
    expect(newGitService().security).toBe("restricted")
    expect(buildProfile("railpack", false)).toBe("restricted")
    expect(buildProfile("dockerfile", true)).toBe("baseline")
  })

  test("a public URL sends its host; a new service listens on 8080", () => {
    const fresh = {
      ...newGitService(),
      url: "https://github.com/acme/site",
      name: "site",
    }
    const change = gitChange({ ...fresh, host: " site.example.com " })
    expect(change.port).toBe(8080)
    expect(change.host).toBe("site.example.com")
    const background = gitChange({ ...fresh, port: "", publicUrl: false })
    expect(background.port).toBeUndefined()
    expect(background.host).toBeUndefined()
  })

  test("the form is one createBuiltService change", () => {
    expect(createGitService(form({}))).toEqual([
      { op: "createBuiltService", ...gitChange(form({})) },
    ])
  })
})

describe("build options in the service's settings", () => {
  const built = (env: Build["env"]) =>
    ({
      name: "site",
      contextDir: "apps/web",
      dockerfile: null,
      env,
    }) as unknown as Build
  const wired = {
    name: "NPM_TOKEN",
    secret: "site-build-env",
    key: "NPM_TOKEN",
  }

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
    expect(first[1]).toMatchObject({
      op: "create",
      object: { kind: "Secret" },
    })
    const literal = { name: "NODE_ENV", secret: null, key: null }
    const build = built([wired, literal])
    expect(
      buildOptionsChanges(build, {
        ...buildOptionsOf(build),
        removed: ["NPM_TOKEN", "NODE_ENV"],
      }),
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
  const builder = { id: "zeabur-builder", name: "Zeabur Builder" }
  const keel = { id: "keel", name: "Keel" }
  const noKeel = { name: "image-updates", existing: false, install: keel }
  const keelUnsupported = {
    name: "image-updates",
    existing: false,
    install: null,
  }
  const zot = "zot.zot.svc:5000"
  test.each([
    [
      "Builder, which brings zot, and Keel, all at once",
      "not-installed",
      [noKeel],
      zot,
      {
        install: [builder, keel],
        keelUnavailable: false,
        registrySetting: false,
      },
    ],
    [
      "the Builder alone",
      "not-installed",
      [],
      zot,
      { install: [builder], keelUnavailable: false, registrySetting: false },
    ],
    [
      "the registry setting, which no component gives",
      "installed",
      [],
      "",
      { install: [], keelUnavailable: false, registrySetting: true },
    ],
    [
      "Keel, with another registry",
      "installed",
      [noKeel],
      "ghcr.io/acme",
      { install: [keel], keelUnavailable: false, registrySetting: false },
    ],
    [
      "Keel that this Kubernetes version cannot install, so nothing opens it",
      "installed",
      [keelUnsupported],
      zot,
      { install: [], keelUnavailable: true, registrySetting: false },
    ],
    ["nothing with a configured registry", "installed", [], "harbor.example.com", undefined],
    ["nothing while unknown", undefined, undefined, undefined, undefined],
  ] as const)("%s", (_, builds, capabilities, registry, needs) => {
    const read = builds && { state: builds, component: builder }
    expect(gitNeeds({ builds: read, capabilities: capabilities as never, registry })).toEqual(
      needs as never,
    )
  })
})

describe("the Builds tab", () => {
  const build = (patch: Partial<Build>) =>
    ({
      name: "site",
      gitRef: "HEAD",
      resolvedRef: null,
      problem: null,
      ...patch,
    }) as Build
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
      startedAt: null,
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

describe("the Dockerfile's path", () => {
  test("is found at the Dockerfile's path inside the directory", () => {
    expect(dockerfileAt("", "")).toBe("Dockerfile")
    expect(dockerfileAt("./apps/web/", "docker/Dockerfile.prod")).toBe(
      "apps/web/docker/Dockerfile.prod",
    )
  })
})

describe("the port of a method", () => {
  test("Railpack starts at 8080; a Dockerfile starts with no network", () => {
    expect(defaultPort("railpack")).toBe("8080")
    expect(defaultPort("dockerfile")).toBe("")
  })
})

describe("the methods offered", () => {
  test("the agent while its builder is ready, or for a service that builds with it", () => {
    expect(offeredMethods(false)).toEqual(["railpack", "dockerfile"])
    expect(offeredMethods(true)).toEqual(["railpack", "dockerfile", "agent"])
    expect(offeredMethods(false, "agent")).toEqual(["railpack", "dockerfile", "agent"])
  })
})

describe("the profile of an agent build", () => {
  test("is Strict when the agent writes the Dockerfile, and Compatible for the folder's own", () => {
    expect(buildProfile("agent", false)).toBe("restricted")
    expect(buildProfile("agent", true)).toBe("baseline")
  })
})

describe("the method of a folder", () => {
  test("is Dockerfile when the folder has one, until the person chooses", () => {
    expect(derivedMethod(false, "railpack", true)).toBe("dockerfile")
    expect(derivedMethod(false, "railpack", false)).toBe("railpack")
    expect(derivedMethod(true, "railpack", true)).toBe("railpack")
    expect(derivedMethod(true, "dockerfile", false)).toBe("dockerfile")
  })
})
