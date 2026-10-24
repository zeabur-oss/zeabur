import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// /logs/history is stubbed (page.route) for each state but the first: local mode runs without
// Zeabur Proxy. ZEABUR_E2E_NAMESPACE, ZEABUR_E2E_SERVICE and ZEABUR_E2E_POD as in ui.spec.ts.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const service = env("ZEABUR_E2E_SERVICE", "talker")
const pod = env("ZEABUR_E2E_POD")
const logsPage = `/projects/${encodeURIComponent(namespace)}/services/Deployment/${service}?tab=logs`
const historyPath = /\/api\/v1\/projects\/[^/]+\/services\/[^/]+\/[^/]+\/logs\/history(\?.*)?$/

function logHistory(state: string, extra: object = {}) {
  return {
    id: `projects/${namespace}/services/deployment/${service}/logs/history`,
    state,
    range: "1h",
    lines: [],
    limit: 1000,
    truncated: false,
    ...extra,
  }
}

async function stub(page: Page, body: object) {
  await page.unroute(historyPath)
  await page.route(historyPath, (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify(body) }),
  )
}

async function openHistory(page: Page) {
  await page.goto(logsPage)
  const toggle = page.getByRole("button", { name: "History", exact: true })
  await toggle.click()
  await expect(toggle).toHaveAttribute("aria-pressed", "true")
  return page.getByRole("tabpanel", { name: "Logs" })
}

test.beforeAll(() => {
  if (!namespace || !pod) throw new Error("ZEABUR_E2E_NAMESPACE and ZEABUR_E2E_POD are not set")
})

test("without Zeabur Proxy, history says how it is turned on", async ({ page }) => {
  const logs = await openHistory(page)
  await expect(logs).toContainText("Log history isn't turned on")
  await expect(logs).toContainText("proxy.enabled")
  // History is every replica's: no replica to pick, nothing to follow.
  await expect(logs.getByRole("button", { name: /^Replica / })).toHaveCount(0)
  await expect(logs.getByRole("button", { name: "Follow" })).toHaveCount(0)
})

test("history shows replaced replicas' lines, oldest first, and keeps the range", async ({
  page,
}) => {
  const gone = `${service}-5d8f7c9b4-gone1`
  await stub(
    page,
    logHistory("available", {
      lines: [
        { time: "2026-10-22T09:00:00Z", pod: gone, container: "talker", message: "from before" },
        { time: "2026-10-22T09:00:01.5Z", pod, container: "talker", message: "error: now" },
      ],
    }),
  )
  const logs = await openHistory(page)
  const lines = logs.getByRole("log", { name: "Log lines" })
  await expect(lines).toContainText("5d8f7c9b4-gone1")
  await expect(lines.getByText("from before")).toBeVisible()
  await expect(logs.getByRole("button", { name: /^Error/ })).toContainText("1")

  const asked = page.waitForRequest((request) => request.url().includes("range=7d"))
  await logs.getByRole("combobox", { name: "Range" }).click()
  await page.getByRole("option", { name: "7d" }).click()
  await asked
})

test("history in each state: not allowed, not answering, cut at the limit", async ({ page }) => {
  await stub(page, logHistory("unreadable"))
  const logs = await openHistory(page)
  await expect(logs).toContainText(/Reading this project's log history needs /)
  await logs.getByRole("button", { name: "Details" }).click()
  await expect(logs).toContainText("get logs.proxy.zeabur.com")

  await stub(page, logHistory("unavailable", { message: "VictoriaLogs did not answer in 30 s" }))
  await page.reload()
  await page.getByRole("button", { name: "History", exact: true }).click()
  await expect(logs).toContainText("The log store did not respond.")
  await expect(logs).toContainText("VictoriaLogs did not answer in 30 s")

  await stub(
    page,
    logHistory("available", {
      limit: 2,
      truncated: true,
      lines: [
        { time: "2026-10-22T09:00:00Z", pod, container: "talker", message: "one" },
        { time: "2026-10-22T09:00:01Z", pod, container: "talker", message: "two" },
      ],
    }),
  )
  await page.reload()
  await page.getByRole("button", { name: "History", exact: true }).click()
  await expect(logs).toContainText("Latest 2 lines")
})
