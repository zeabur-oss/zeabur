import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// The terminal dock under every page. Runs where ui.spec.ts runs, with its variables. The kubectl
// terminal needs a server that offers one: Zeabur in a cluster, or the demo server with
// ZEABUR_DEMO_TOOLS=1; elsewhere its test checks that none is offered.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const healthy = env("ZEABUR_E2E_SERVICE", "talker")
const failing = env("ZEABUR_E2E_FAILING", "crasher")
const project = `/projects/${encodeURIComponent(namespace)}`

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

const dock = (page: Page) => page.getByRole("region", { name: "Terminal" })
const screen = (page: Page) => dock(page).getByRole("group", { name: "Terminal" })
const connected = (page: Page) => dock(page).getByRole("status").filter({ hasText: "Connected" })
const toggle = (page: Page) =>
  page.getByRole("navigation").getByRole("button", { name: /^Terminal( \d+)?$/ })
const inTerminal = (page: Page) => page.evaluate(() => !!document.activeElement?.closest(".xterm"))

async function openShell(page: Page, service = healthy) {
  await page.goto(`${project}/services/Deployment/${service}?tab=logs`)
  await page
    .getByRole("tabpanel", { name: "Logs" })
    .getByRole("button", { name: "Terminal", exact: true })
    .click()
  await expect(dock(page).getByRole("tab", { name: service })).toHaveAttribute(
    "aria-selected",
    "true",
  )
}

async function search(page: Page, words: string) {
  await page.getByRole("button", { name: "Search" }).click()
  await page.getByRole("combobox", { name: /Jump to/ }).fill(words)
}

const toolsAvailable = async (page: Page) => {
  const response = await page.request.get(`/api/v1/projects/${namespace}/tools`)
  return response.ok() && ((await response.json()) as { available: boolean }).available
}

test("a service's terminal lives on while the person moves between services", async ({ page }) => {
  const sockets: string[] = []
  page.on("websocket", (socket) => {
    if (socket.url().includes("/exec")) sockets.push(socket.url())
  })
  await openShell(page)
  await expect(connected(page)).toBeVisible({ timeout: 30_000 })
  // The keyboard is in the terminal once it opens.
  await expect.poll(() => inTerminal(page)).toBe(true)
  await page.keyboard.type("echo zeabur-dock-$((6 * 7))\n")
  await expect(screen(page)).toContainText("zeabur-dock-")

  const sections = page.getByRole("navigation", { name: "Project sections" })
  await sections.getByRole("link", { name: new RegExp(`^${failing}\\b`) }).click()
  await expect(page.getByRole("heading", { level: 1, name: failing })).toBeVisible()
  await expect(screen(page)).toContainText("zeabur-dock-")
  await sections.getByRole("link", { name: new RegExp(`^${healthy}\\b`) }).click()
  await expect(page.getByRole("heading", { level: 1, name: healthy })).toBeVisible()
  await expect(screen(page)).toContainText("zeabur-dock-")
  await expect(connected(page)).toBeVisible()
  // One connection all along: moving around neither closed nor reopened it.
  expect(sockets).toHaveLength(1)

  // The header's Terminal opens another tab; asking again shows the one already open.
  await page
    .locator("main header")
    .first()
    .getByRole("button", { name: "Terminal", exact: true })
    .click()
  await expect(dock(page).getByRole("tab")).toHaveCount(1)
})

test("the dock folds to a bar with Ctrl+`, keeps Escape for the shell, and resizes", async ({
  page,
}) => {
  await openShell(page)
  await expect(connected(page)).toBeVisible({ timeout: 30_000 })
  await expect.poll(() => inTerminal(page)).toBe(true)

  // Escape belongs to the program in the terminal (vi, less).
  await page.keyboard.press("Escape")
  await expect(dock(page)).toBeVisible()

  await page.keyboard.press("Control+Backquote")
  await expect(dock(page)).toBeHidden()
  const bar = page.getByRole("button", { name: /^1 terminal/ })
  await expect(bar).toBeVisible()
  await expect(bar).toBeFocused()
  // A backtick alone still types.
  await page.keyboard.press("Control+Backquote")
  await expect(dock(page)).toBeVisible()
  await expect.poll(() => inTerminal(page)).toBe(true)
  await page.keyboard.type("echo `printf zeabur-tick`\n")
  await expect(screen(page)).toContainText("zeabur-tick")

  // Still for the drag: the dock has risen all the way.
  await page.waitForFunction(
    () => document.querySelector('[data-slot="dock"]')?.getAnimations().length === 0,
  )
  const edge = dock(page).getByRole("separator", { name: "Terminal height" })
  const before = Number(await edge.getAttribute("aria-valuenow"))
  const height = (await dock(page).boundingBox())!.height
  await edge.focus()
  await page.keyboard.press("ArrowUp")
  await page.keyboard.press("ArrowUp")
  await expect(edge).toHaveAttribute("aria-valuenow", String(before + 32))
  expect((await dock(page).boundingBox())!.height).toBeCloseTo(height + 32, 0)

  const box = (await edge.boundingBox())!
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 - 40, { steps: 4 })
  await page.mouse.up()
  await expect(edge).toHaveAttribute("aria-valuenow", String(before + 72))

  // The height is the viewer's own, kept for the next visit.
  await page.reload()
  await toggle(page).click()
  await expect(dock(page).getByRole("separator", { name: "Terminal height" })).toHaveAttribute(
    "aria-valuenow",
    String(before + 72),
  )
})

test("a terminal closed when its credential expires keeps its tab and reopens", async ({
  page,
}) => {
  const expiresAt = new Date(Date.UTC(2030, 0, 1, 15, 34)).toISOString()
  await page.route("**/api/v1/session", async (route) => {
    const response = await route.fetch()
    await route.fulfill({ response, json: { ...(await response.json()), expiresAt } })
  })
  let opened = 0
  await page.routeWebSocket(/\/exec/, (socket) => {
    opened += 1
    socket.send(JSON.stringify({ type: "ready", container: healthy }))
    socket.send(Buffer.from("$ "))
    if (opened === 1) setTimeout(() => socket.close({ code: 4000, reason: "expired" }), 400)
    if (opened === 2) setTimeout(() => socket.close({ code: 4001, reason: "idle" }), 400)
  })
  await openShell(page)
  const panel = dock(page).getByRole("tabpanel")
  await expect(panel.getByText("The credential in this terminal has expired.")).toBeVisible()
  await expect(dock(page)).toBeVisible()
  await expect(dock(page).getByRole("tab", { name: healthy })).toBeVisible()
  await panel.getByRole("button", { name: "Reopen" }).click()
  await expect(panel.getByText("Closed after 15 minutes without input or output.")).toBeVisible()
  await expect(dock(page)).toBeVisible()
  await panel.getByRole("button", { name: "Reopen" }).click()
  await expect(connected(page)).toBeVisible()

  // Folded, the bar says when the first of the open terminals will close.
  await page.keyboard.press("Control+Backquote")
  const clock = new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(new Date(expiresAt))
  await expect(page.getByRole("button", { name: /^1 terminal/ })).toContainText(
    `Credential expires at ${clock}`,
  )
})

test("an old link to Logs' terminal view opens the dock", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${healthy}?tab=logs&mode=terminal`)
  await expect(page).toHaveURL(
    (url) => url.searchParams.get("tab") === "logs" && !url.searchParams.has("mode"),
  )
  await expect(dock(page).getByRole("tab", { name: healthy })).toHaveAttribute(
    "aria-selected",
    "true",
  )
})

test("the kubectl terminal opens through a review the first time and reuses its pod after", async ({
  page,
}) => {
  await page.goto(project)
  if (!(await toolsAvailable(page))) {
    // Zeabur on the person's computer holds no token to hand over: no kubectl terminal is offered.
    await search(page, "kubectl")
    await expect(page.getByRole("option", { name: "Open kubectl terminal" })).toHaveCount(0)
    await page.keyboard.press("Escape")
    await toggle(page).click()
    await expect(dock(page)).toContainText("No terminal open")
    await expect(dock(page).getByRole("button", { name: "Open kubectl terminal" })).toHaveCount(0)
    return
  }

  await search(page, "kubectl")
  await page.getByRole("option", { name: "Open kubectl terminal" }).click()
  const tab = dock(page).getByRole("tab", { name: `kubectl ${namespace}` })
  await expect(tab).toHaveAttribute("aria-selected", "true")
  const dialog = page.getByRole("dialog", { name: "Open kubectl terminal" })
  const reused = await dialog
    .waitFor({ timeout: 5_000 })
    .then(() => false)
    .catch(() => true)
  if (!reused) {
    await expect(dialog).toContainText("kubectl and helm act as")
    await dialog.getByRole("button", { name: "Open kubectl terminal" }).click()
  }
  await expect(connected(page)).toBeVisible({ timeout: 60_000 })
  await expect.poll(() => inTerminal(page)).toBe(true)

  // Closed and asked for again: the running pod is used, with no review.
  await tab.press("Delete")
  await expect(tab).toHaveCount(0)
  await search(page, "kubectl")
  await page.getByRole("option", { name: "Open kubectl terminal" }).click()
  await expect(connected(page)).toBeVisible({ timeout: 30_000 })
  await expect(page.getByRole("dialog")).toHaveCount(0)
})

test("a command on a page is text to copy; the cluster page opens the cluster's kubectl terminal", async ({
  page,
}) => {
  // crasher has no Service, so its Networking tab shows the command that adds one.
  await page.goto(`${project}/services/Deployment/${failing}?tab=networking`)
  const panel = page.getByRole("tabpanel", { name: "Networking" })
  const command = `kubectl -n ${namespace} expose deployment ${failing}`
  await expect(panel.getByText(command, { exact: true })).toBeVisible()
  await expect(panel.getByRole("button", { name: "Copy" })).toBeVisible()
  await expect(panel.getByRole("button", { name: /terminal/ })).toHaveCount(0)

  await page.goto("/cluster")
  const open = page.getByRole("button", { name: "Open kubectl terminal" })
  const response = await page.request.get("/api/v1/cluster/tools")
  const available =
    response.status() === 403 ||
    (response.ok() && ((await response.json()) as { available: boolean }).available)
  if (!available) {
    await expect(open).toHaveCount(0)
    return
  }
  await open.click()
  await expect(dock(page).getByRole("tab", { name: /^kubectl / })).toHaveAttribute(
    "aria-selected",
    "true",
  )
})

test("on a phone the dock is a sheet over the whole screen", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await openShell(page)
  await expect
    .poll(async () => {
      const box = (await dock(page).boundingBox())!
      return [box.x, box.y, box.width, Math.round(box.height)]
    })
    .toEqual([0, 0, 390, 844])
  await dock(page).getByRole("button", { name: "Hide terminal" }).click()
  await expect(dock(page)).toBeHidden()
  await expect(page.getByRole("button", { name: /^1 terminal/ })).toBeVisible()
})
