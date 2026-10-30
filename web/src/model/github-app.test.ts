import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  appName,
  connectStep,
  grantChoices,
  grantedRepository,
  nameProblem,
  repositoriesOf,
  webhookProblem,
  withProject,
} from "@/model/github-app"

describe("the cluster's GitHub App", () => {
  test("its name starts from the cluster's and fits GitHub's limit", () => {
    expect(appName(" shop-prod ")).toBe("Zeabur shop-prod")
    expect(nameProblem(appName("a".repeat(60)))).toBeUndefined()
    expect(nameProblem("  ")).toBe("empty")
    expect(nameProblem("x".repeat(35))).toBe("long")
  })

  test("a webhook URL is optional and HTTPS", () => {
    expect(webhookProblem("")).toBeUndefined()
    expect(webhookProblem("https://hooks.example.com/hooks/github-app")).toBeUndefined()
    expect(webhookProblem("http://hooks.example.com")).toBe("invalid")
  })

  test("a project picks from every installation's repositories, by name", () => {
    const repos = repositoriesOf({
      installations: [
        {
          id: 2,
          account: "zeta",
          refreshedAt: "",
          repositories: [{ fullName: "zeta/app", defaultBranch: "main", private: true }],
        },
        {
          id: 1,
          account: "acme",
          refreshedAt: "",
          repositories: [{ fullName: "acme/web", defaultBranch: "trunk", private: false }],
        },
      ],
    })
    expect(repos.map((r) => [r.fullName, r.installationId])).toEqual([
      ["acme/web", 1],
      ["zeta/app", 2],
    ])
  })

  test("a grant keeps namespaces it names that are not projects", () => {
    expect(grantChoices(["shop", "blog"], ["legacy", "shop"])).toEqual(["blog", "legacy", "shop"])
  })
})

const installation = (
  id: number,
  namespaces: string[],
  everyNamespace = false,
): Schemas.GitHubInstallation => ({
  id,
  account: `account-${id}`,
  accountType: "Organization",
  repositorySelection: "all",
  settingsUrl: "",
  namespaces,
  everyNamespace,
})

const cluster = (over: Partial<Schemas.GitHubApp>): Schemas.GitHubApp => ({
  namespace: "builder-system",
  canChange: "allowed",
  webUrl: "https://github.com",
  unreadable: [],
  app: {
    htmlUrl: "https://github.com/apps/z",
    installUrl: "https://github.com/apps/z/installations/new",
    webhook: false,
  },
  installations: [],
  ...over,
})

describe("connecting GitHub from the Git form", () => {
  test("without an App, someone who may make one makes it; anyone else asks", () => {
    expect(connectStep(cluster({ app: undefined }), "shop")?.step).toBe("create")
    expect(connectStep(cluster({ app: undefined, canChange: "denied" }), "shop")?.step).toBe("ask")
    const hidden = cluster({
      app: undefined,
      unreadable: [{ resource: "secrets" } as Schemas.UnreadableView],
    })
    expect(connectStep(hidden, "shop")?.step).toBe("ask")
  })

  test("an App installed nowhere is installed on GitHub first", () => {
    expect(connectStep(cluster({}), "shop")).toEqual({
      step: "install",
      installUrl: "https://github.com/apps/z/installations/new",
    })
  })

  test("the project is granted the installations it lacks, never one open to every project", () => {
    const data = cluster({
      installations: [
        installation(1, ["shop"]),
        installation(2, ["blog"]),
        installation(3, [], true),
      ],
    })
    const step = connectStep(data, "shop")
    expect(step?.step).toBe("grant")
    expect(step?.step === "grant" && step.installations.map((i) => i.id)).toEqual([2])
    expect(withProject(["blog"], "shop")).toEqual(["blog", "shop"])
  })

  test("only someone who may change the App grants", () => {
    const data = cluster({ canChange: "denied", installations: [installation(2, ["blog"])] })
    expect(connectStep(data, "shop")?.step).toBe("ask")
  })

  test("installations GitHub did not list are a problem to look at, not an empty list", () => {
    expect(connectStep(cluster({ installations: undefined }), "shop")?.step).toBe("problem")
  })
})

describe("a typed GitHub URL of a granted repository", () => {
  const repos = repositoriesOf({
    installations: [
      {
        id: 7,
        account: "acme",
        refreshedAt: "",
        repositories: [{ fullName: "acme/Web", defaultBranch: "main", private: true }],
      },
    ],
  })

  test("is read through the App, however it is written", () => {
    for (const url of [
      "https://github.com/acme/web",
      "https://github.com/acme/web.git",
      "git@github.com:acme/web.git",
    ])
      expect(grantedRepository(repos, url)?.installationId).toBe(7)
  })

  test("on another host or another repository is not", () => {
    expect(grantedRepository(repos, "https://gitlab.com/acme/web")).toBeUndefined()
    expect(grantedRepository(repos, "https://github.com/acme/api")).toBeUndefined()
  })
})
