import { describe, expect, test } from "bun:test"

import {
  appName,
  grantChoices,
  nameProblem,
  repositoriesOf,
  webhookProblem,
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
