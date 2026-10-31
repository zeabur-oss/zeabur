import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// States a test cluster does not reach on its own, stubbed with page.route. ZEABUR_E2E_NAMESPACE
// and ZEABUR_E2E_SERVICE as in ui.spec.ts.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const service = env("ZEABUR_E2E_SERVICE", "talker")
const project = `/projects/${encodeURIComponent(namespace)}`
const servicePage = `${project}/services/Deployment/${service}`

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

async function stub(page: Page, path: RegExp, body: object, status = 200) {
  await page.route(path, (route) =>
    route.fulfill({ status, contentType: "application/json", body: JSON.stringify(body) }),
  )
}

async function change<T>(page: Page, path: RegExp, edit: (body: T) => T) {
  await page.route(path, async (route) => {
    const response = await route.fetch()
    const body = (await response.json()) as T
    await route.fulfill({ response, json: edit(body) })
  })
}

const planPath = /\/api\/v1\/projects\/[^/]+\/plan$/
const quota = {
  type: "/problems/quota-exceeded",
  title: "The project's limit does not allow this",
  status: 403,
  detail: `deployments.apps "${service}" is forbidden: exceeded quota: compute`,
  fix: "Ask for less, free some of what the project uses, or ask a cluster administrator to raise the limit compute.",
  kind: "quota-exceeded",
  summary: { key: "problem.summary.quota-exceeded", params: { name: service } },
  why: {
    key: "problem.why.quota-exceeded",
    params: { quota: "compute", resource: "requests.cpu", hard: "4", used: "3500m" },
  },
  fixKey: { key: "problem.quotaExceeded.fix.scale", params: { quota: "compute", object: service } },
  evidence: [
    { kind: "ResourceQuota", name: "compute", fieldPath: "status.hard.requests.cpu", value: "4" },
    {
      kind: "ResourceQuota",
      name: "compute",
      fieldPath: "status.used.requests.cpu",
      value: "3500m",
    },
  ],
  kubernetes: {
    code: 403,
    reason: "Forbidden",
    message: `deployments.apps "${service}" is forbidden: exceeded quota: compute, requested: requests.cpu=1, used: requests.cpu=3500m, limited: requests.cpu=4`,
  },
}

const policy = {
  type: "/problems/policy-denied",
  title: "A policy in the cluster refused this",
  status: 403,
  detail:
    'admission webhook "require-labels.example.com" denied the request: label team is required',
  fix: "Change what it names, or ask whoever runs the policy.",
  kind: "policy-denied",
  summary: { key: "problem.summary.policy-denied", params: { name: service } },
  why: {
    key: "problem.why.policy-denied",
    params: { policy: "require-labels.example.com", message: "label team is required" },
  },
  fixKey: { key: "problem.policyDenied.fix.change" },
  kubernetes: { code: 400, reason: "BadRequest", message: "label team is required" },
}

async function planRestart(page: Page) {
  await page.goto(servicePage)
  await page.getByRole("button", { name: "Restart", exact: true }).click()
  return page.getByRole("dialog", { name: `Restart ${service}` })
}

// What the problem says, and what it folds, is in src/components/refused.test.ts.
test("a project's limit shows in the review, with its evidence and the raw answer on request", async ({
  page,
}) => {
  await stub(page, planPath, quota, 403)
  const dialog = await planRestart(page)
  const problem = dialog.getByRole("alert")
  await expect(problem).toContainText(`The change to ${service} would go over the project's limit.`)
  await problem.getByRole("button", { name: "Evidence" }).click()
  await expect(problem).toContainText("ResourceQuota/compute status.used.requests.cpu")
  await problem.getByRole("button", { name: "Raw error" }).click()
  await expect(problem).toContainText("exceeded quota: compute")
  await expect(
    problem.getByRole("link", { name: /What this kind of problem means/ }),
  ).toHaveAttribute("href", "/problems/quota-exceeded")
})

test("a policy's refusal quotes the policy's own words", async ({ page }) => {
  await stub(page, planPath, policy, 403)
  const dialog = await planRestart(page)
  const problem = dialog.getByRole("alert")
  await expect(problem).toContainText(`A policy in the cluster refused the change to ${service}.`)
  await expect(problem).toContainText("The policy require-labels.example.com answered:")
  await expect(problem.getByRole("blockquote")).toHaveText("label team is required")
})

test("a pod that runs again after a recent crash keeps the service failing, in plain words", async ({
  page,
}) => {
  const finished = new Date(Date.now() - 4 * 60_000).toISOString()
  type Document = {
    services: {
      kind: string
      name: string
      status: object
      pods: { ready: boolean; restarts: number; health: string; containerStatuses?: object[] }[]
    }[]
  }
  await change<Document>(page, /\/api\/v1\/projects\/[^/]+$/, (body) => {
    const target = body.services.find((s) => s.kind === "Deployment" && s.name === service)
    const pod = target?.pods[0]
    if (target && pod) {
      // What the health rule reports for a Ready pod whose last run ended with an error.
      target.status = { health: "fail", word: "failing", reason: "Error" }
      Object.assign(pod, {
        ready: true,
        phase: "Running",
        reason: null,
        health: "fail",
        restarts: 5,
        containerStatuses: [
          {
            name: service,
            ready: true,
            restarts: 5,
            lastTermination: { exitCode: 1, reason: "Error", finished },
          },
        ],
      })
    }
    return body
  })
  await page.goto(servicePage)
  const status = page.getByRole("region", { name: "Status" })
  await expect(status.getByRole("heading", { level: 2 })).toContainText("Keeps crashing")
  await expect(status.getByText("Running, 5 restarts, last exit 1, 4 minutes ago")).toBeVisible()
})

test("project settings say who to ask where the hint says no", async ({ page }) => {
  test.skip(namespace.startsWith("kube-"), "a system namespace is never a project")
  const namespacePath = /\/api\/v1\/projects\/[^/]+\/namespace$/
  await change<object>(page, namespacePath, (body) => ({
    ...body,
    adopted: true,
    canChange: "denied",
  }))
  await page.goto(`${project}/settings`)
  await expect(page.getByText("You can see these but not change them.")).toBeVisible()
})

test("each problem type has a page that explains its rule", async ({ page }) => {
  await page.goto("/problems/quota-exceeded")
  await expect(
    page.getByRole("heading", { level: 1, name: "This would go over the project's limit" }),
  ).toBeVisible()
  await expect(page.getByText(/A project can have limits on what all its services/)).toBeVisible()
  await expect(page.getByRole("link", { name: /Resource Quotas/ })).toHaveAttribute(
    "href",
    "https://kubernetes.io/docs/concepts/policy/resource-quotas/",
  )
  await page.goto("/problems/no-such-kind")
  await expect(page.getByText("This page doesn't exist")).toBeVisible()
})

test("the project page shows its services while the suggestions are still being read", async ({
  page,
}) => {
  let release = () => {}
  const held = new Promise<void>((resolve) => (release = resolve))
  await page.route(/\/api\/v1\/projects\/[^/]+\/gaps$/, async (route) => {
    await held
    await route.continue()
  })
  await page.goto(`${project}?view=list`)
  const services = page
    .getByRole("table", { name: "Services" })
    .or(page.getByRole("list", { name: "Services" }))
  await expect(services).toBeVisible()
  const waiting = page.locator("main header").first().getByText("Looking for suggestions…")
  await expect(waiting).toBeVisible()
  release()
  await expect(waiting).toHaveCount(0)
})
