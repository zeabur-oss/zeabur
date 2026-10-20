import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// Usage, history and the metrics store setting in each state the API reports. The answers of
// /usage, /history and /settings/metrics-store are stubbed (page.route), so every state shows on
// any server: a cluster without metrics-server or a store, or the demo. Everything else is the
// server's. ZEABUR_E2E_NAMESPACE, ZEABUR_E2E_SERVICE and ZEABUR_E2E_POD as in ui.spec.ts.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const service = env("ZEABUR_E2E_SERVICE", "talker")
const pod = env("ZEABUR_E2E_POD")
// Usage and history are on the service's Metrics tab.
const page_ = `/projects/${encodeURIComponent(namespace)}/services/Deployment/${service}?tab=metrics`
const STORE = "observability/vmsingle-vm-victoria-metrics-k8s-stack:8428"
const MiB = 2 ** 20

const forbidden = (resource: string) => ({
  resource,
  kubernetes: { code: 403, reason: "Forbidden", message: `${resource} is forbidden` },
})

function usage(state: string, pods: { pod: string; cpu: number; memory: number }[] = []) {
  return {
    id: `projects/${namespace}/usage`,
    state,
    window: "15s",
    unreadable: state === "unreadable" ? [forbidden("pods.metrics.k8s.io")] : [],
    pods: pods.map((p) => ({
      origin: { apiVersion: "metrics.k8s.io/v1beta1", kind: "PodMetrics", name: p.pod },
      pod: p.pod,
      cpuMillicores: p.cpu,
      memoryBytes: p.memory,
      containers: [],
    })),
  }
}

function history(state: string, extra: object = {}) {
  const times = Array.from({ length: 241 }, (_, i) => 1_760_000_000 + i * 15)
  return {
    id: `projects/${namespace}/services/deployment/${service}/history`,
    state,
    source: { service: STORE },
    range: "1h",
    step: 15,
    times: state === "available" ? times : [],
    cpu: [],
    memory: [],
    unreadable: [],
    ...extra,
  }
}

const series = (scale: number) => [
  { pod, values: Array.from({ length: 241 }, (_, i) => scale * (1 + Math.sin(i / 20) / 4)) },
]

async function stub(page: Page, path: RegExp, body: object) {
  await page.route(path, (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify(body) }),
  )
}

const usagePath = /\/api\/v1\/projects\/[^/]+\/usage$/
const historyPath = /\/api\/v1\/projects\/[^/]+\/services\/[^/]+\/[^/]+\/history(\?.*)?$/

test.beforeAll(() => {
  if (!namespace || !pod) throw new Error("ZEABUR_E2E_NAMESPACE and ZEABUR_E2E_POD are not set")
})

test("live usage per replica against what it reserves, and a replica without a sample", async ({
  page,
}) => {
  await stub(page, usagePath, usage("available", [{ pod, cpu: 42, memory: 120 * MiB }]))
  await stub(page, historyPath, history("not-configured"))
  await page.goto(page_)
  const panel = page.getByRole("region", { name: "Usage" })
  await expect(panel.getByText("Live")).toBeVisible()
  const row = panel.getByRole("row").filter({ hasText: pod })
  await expect(row).toContainText("42m")
  await expect(row).toContainText("120 MiB")
})

test("without metrics-server, the reserved amounts stay and Install is offered", async ({
  page,
}) => {
  await stub(page, usagePath, usage("not-installed"))
  await stub(page, historyPath, history("not-configured"))
  await page.goto(page_)
  const panel = page.getByRole("region", { name: "Usage" })
  await expect(panel).toContainText("Live usage needs metrics-server.")
  await expect(panel.getByRole("link", { name: "Install" })).toHaveAttribute(
    "href",
    /\/cluster\/components\?component=metrics%2Fmetrics-server/,
  )
  await expect(panel).toContainText(/Each replica \(×\d+\)/)
  // The services table has no usage columns to fill.
  await page.goto(`/projects/${encodeURIComponent(namespace)}`)
  const table = page.getByRole("table", { name: /Services/ })
  await expect(table).toBeVisible()
  await expect(table.getByRole("columnheader", { name: "Memory" })).toHaveCount(0)
})

test("a refused read of live usage is a quiet line with the missing right", async ({ page }) => {
  await stub(page, usagePath, usage("unreadable"))
  await stub(page, historyPath, history("not-configured"))
  await page.goto(page_)
  const panel = page.getByRole("region", { name: "Usage" })
  await expect(panel).toContainText("Seeing live usage")
  await panel.getByRole("button", { name: "Details" }).click()
  await expect(panel).toContainText("list pods.metrics.k8s.io")
})

test("the services table shows each service's live CPU and memory", async ({ page }) => {
  await stub(page, usagePath, usage("available", [{ pod, cpu: 1234, memory: 3 * 2 ** 30 }]))
  await page.goto(`/projects/${encodeURIComponent(namespace)}`)
  const row = page
    .getByRole("table", { name: /Services/ })
    .getByRole("row")
    .filter({ hasText: service })
    .first()
  await expect(row).toContainText("1.23 cores")
  await expect(row).toContainText("3 GiB")
})

test("history in each state: no store, not allowed, not answering, no samples", async ({
  page,
}) => {
  await stub(page, usagePath, usage("not-installed"))
  const panel = page.getByRole("region", { name: "History" })

  await stub(page, historyPath, history("not-configured"))
  await page.goto(page_)
  await expect(panel).toContainText("History needs a metrics store.")
  await expect(panel.getByRole("link", { name: "Choose another store" })).toHaveAttribute(
    "href",
    "/settings",
  )

  await page.unroute(historyPath)
  await stub(page, historyPath, {
    ...history("unreadable"),
    unreadable: [
      forbidden("services/proxy (observability/vmsingle-vm-victoria-metrics-k8s-stack)"),
    ],
  })
  await page.reload()
  await expect(panel).toContainText("Seeing usage history")
  await panel.getByRole("button", { name: "Details" }).click()
  await expect(panel).toContainText(
    "get services/proxy (observability/vmsingle-vm-victoria-metrics-k8s-stack)",
  )

  await page.unroute(historyPath)
  await stub(page, historyPath, { ...history("unavailable"), message: "no endpoints available" })
  await page.reload()
  await expect(panel).toContainText("The metrics store did not answer.")

  await page.unroute(historyPath)
  await stub(page, historyPath, history("available", { cpu: [], memory: [] }))
  await page.reload()
  await expect(panel).toContainText(`No samples for ${service} in the last hour.`)
})

test("history draws a line per replica or the total, and keeps the choice in the URL", async ({
  page,
}) => {
  await stub(page, usagePath, usage("not-installed"))
  await stub(
    page,
    historyPath,
    history("available", {
      cpu: series(40),
      memory: series(100 * MiB),
      restarts: [{ pod, at: 1_760_001_800, oomKilled: true }],
    }),
  )
  await page.goto(page_)
  const panel = page.getByRole("region", { name: "History" })
  // The chart's own chunk has loaded and drawn the replica's line in each chart.
  await expect(panel.locator(".recharts-line-curve")).toHaveCount(2)
  await expect(panel.getByText(pod, { exact: true })).toBeVisible()
  await expect(panel).toContainText(`From ${STORE}`)
  await expect(panel).toContainText("Out of memory")
  await panel.getByRole("button", { name: "Total" }).click()
  await expect(page).toHaveURL(/split=total/)
  await expect(panel.getByText("Total", { exact: true }).last()).toBeVisible()
  await panel.getByRole("button", { name: "24h" }).click()
  await expect(page).toHaveURL(/range=24h/)
})

test("the metrics store setting says where it is kept, who may change it, and saves", async ({
  page,
}) => {
  const setting = {
    id: "settings/metrics-store",
    service: STORE,
    default: STORE,
    source: "default",
    storedIn: { apiVersion: "v1", kind: "ConfigMap", namespace: "zeabur", name: "zeabur-settings" },
    canChange: "allowed",
    state: "not-configured",
    unreadable: [],
  }
  let saved: unknown
  await page.route(/\/api\/v1\/settings\/metrics-store$/, async (route) => {
    if (route.request().method() === "PUT") {
      saved = route.request().postDataJSON()
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          ...setting,
          service: "monitoring/prometheus-operated:9090",
          source: "setting",
          state: "available",
        }),
      })
    } else {
      await route.fulfill({ contentType: "application/json", body: JSON.stringify(setting) })
    }
  })
  await page.goto("/settings")
  const panel = page.getByRole("region", { name: "Usage history" })
  await expect(panel).toContainText("Not found")
  await expect(panel).toContainText("Usually only cluster administrators can change this.")
  await panel.getByRole("button", { name: "Where this is kept" }).hover()
  // Base UI's tooltip popup carries no tooltip role.
  await expect(page.getByText("ConfigMap zeabur-settings in the namespace zeabur")).toBeVisible()
  const input = panel.getByRole("textbox", { name: "History source" })
  await input.fill("monitoring/prometheus-operated:9090")
  await panel.getByRole("button", { name: "Save" }).click()
  await expect(panel).toContainText("Responding")
  expect(saved).toEqual({ service: "monitoring/prometheus-operated:9090" })
  await expect(panel.getByRole("button", { name: "Use the default" })).toBeVisible()
})
