import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { actionOf, describe as describePlan, failureOf } from "@/components/plan/describe"
import { projectGlyphs } from "@/components/project-icon"
import type { Translate } from "@/i18n"
import { en } from "@/i18n/en"
import {
  kubectlLabel,
  mayChange,
  otherNamespaces,
  patchRole,
  projectIcons,
  projectTitle,
} from "@/model/project"
import { destinationOf, weigh } from "@/model/risk"

const english: Translate = (key, ...args) => {
  const message = en[key] as string | ((...a: unknown[]) => string)
  return typeof message === "function" ? message(...args) : message
}

const summary = (
  name: string,
  more: Partial<Schemas["ProjectSummary"]> = {},
): Schemas["ProjectSummary"] => ({
  id: `projects/${name}`,
  origin: { apiVersion: "v1", kind: "Namespace", name },
  name,
  adopted: false,
  system: false,
  ...more,
})

const empty = { objects: [], impact: [], warnings: [] }

describe("the list of namespaces to adopt", () => {
  const items = [
    summary("shop", { adopted: true }),
    summary("blog"),
    summary("kube-system", { system: true }),
  ]

  test("lists only namespaces that are not projects, system ones on request and never adoptable", () => {
    expect(otherNamespaces(items, false)).toStrictEqual([
      { name: "blog", icon: undefined, adoptable: true },
    ])
    expect(otherNamespaces(items, true).map((n) => [n.name, n.adoptable])).toStrictEqual([
      ["blog", true],
      ["kube-system", false],
    ])
  })

  test("names a project by its display name, else its namespace", () => {
    expect(projectTitle(summary("shop", { displayName: "Web shop" }), "shop")).toBe("Web shop")
    expect(projectTitle(summary("shop"), "shop")).toBe("shop")
    expect(projectTitle(undefined, "shop")).toBe("shop")
  })
})

describe("adopting and letting go, as a plan", () => {
  test("letting go says nothing in the namespace is deleted, and adopting that nothing changes", () => {
    const unadopt = describePlan(english, { changes: [{ op: "unadopt" }] }, empty, () => undefined)
    expect(unadopt.lines.join(" ")).toMatch(/Nothing in the namespace is deleted/)
    expect(unadopt.action).toBe("Stop showing as project")
    const adopt = describePlan(english, { changes: [{ op: "adopt" }] }, empty, () => undefined)
    expect(adopt.lines.join(" ")).toMatch(/Nothing in it changes/)
    expect(actionOf(english, { changes: [{ op: "details" }] })).toBe("Save details")
  })

  test("adopting and details are light; letting go is confirmed in full", () => {
    const none = () => undefined
    expect(weigh([{ op: "adopt" }], none).risk).toBe("low")
    expect(weigh([{ op: "details", displayName: "Shop" }], none).risk).toBe("low")
    expect(weigh([{ op: "unadopt" }], none).risk).toBe("normal")
  })

  test("after the change, View goes to the project, the list or the settings", () => {
    expect(destinationOf("shop", [{ op: "adopt" }], undefined)).toBe("/projects/shop")
    expect(destinationOf("shop", [{ op: "unadopt" }], undefined)).toBe("/projects")
    expect(destinationOf("shop", [{ op: "details" }], undefined)).toBe("/projects/shop/settings")
  })

  test("a system namespace's refusal is told by its type, and reviewing again cannot help", () => {
    const f = failureOf(english, "en", {
      type: "/problems/system-namespace",
      title: "Kubernetes keeps this namespace for itself",
      status: 422,
      detail: "kube-system is a namespace Kubernetes keeps for its own components.",
      fix: en["problem.fix.system-namespace"],
      kind: "system-namespace",
      summary: { key: "problem.summary.system-namespace", params: { name: "kube-system" } },
    })
    expect(f.title).toBe("kube-system belongs to Kubernetes itself.")
    expect(f.review).toBe(false)
  })
})

describe("without the right to patch the namespace", () => {
  test("the details are read-only only when the rules say no", () => {
    expect(mayChange({ canChange: "denied" })).toBe(false)
    expect(mayChange({ canChange: "allowed" })).toBe(true)
    // Another authorizer may allow it: the plan's dry run decides.
    expect(mayChange({ canChange: "unknown" })).toBe(true)
  })

  test("the kubectl commands add and remove the one label", () => {
    expect(kubectlLabel("shop")).toStrictEqual({
      adopt: "kubectl label namespace shop zeabur.com/project=true",
      unadopt: "kubectl label namespace shop zeabur.com/project-",
    })
  })

  test("the role to ask for patches this namespace only, and the help says what else it allows", () => {
    const yaml = patchRole("shop", "system:serviceaccount:shop:alice")
    expect(yaml).toMatch(/kind: ClusterRole\n/)
    expect(yaml).toMatch(
      /resources: \["namespaces"\]\n\s+resourceNames: \["shop"\]\n\s+verbs: \["get", "patch"\]/,
    )
    expect(yaml).toMatch(/kind: ClusterRoleBinding/)
    expect(yaml).toMatch(/kind: ServiceAccount\n\s+name: "alice"\n\s+namespace: "shop"/)
    expect(en["access.alsoLabels"]).toMatch(/pod-security\.kubernetes\.io/)
  })
})

test("every built-in icon has a glyph, in the picker's order", () => {
  expect<string[]>([...projectIcons]).toStrictEqual(Object.keys(projectGlyphs))
})
