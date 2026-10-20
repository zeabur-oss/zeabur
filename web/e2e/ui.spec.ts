import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// Runs on a real cluster (scripts/e2e-ui.sh) or the demo server. ZEABUR_E2E_URL: see
// playwright.config.ts.
//
//   ZEABUR_E2E_NAMESPACE     the project
//   ZEABUR_E2E_POD           a pod of the healthy Deployment, logging ZEABUR_E2E_LOG_LINE
//   ZEABUR_E2E_SERVICE       that Deployment                       (default talker)
//   ZEABUR_E2E_LOG_LINE      text its log contains                 (default zeabur-e2e-line)
//   ZEABUR_E2E_FAILING       a crash-looping Deployment            (default crasher)
//   ZEABUR_E2E_SECRET        a Secret the healthy Deployment reads (default talker-secret)
//   ZEABUR_E2E_SECRET_VALUE  one of its values                     (default zeabur-e2e-secret)
//
// The demo server: ZEABUR_E2E_NAMESPACE=shop ZEABUR_E2E_SERVICE=api
// ZEABUR_E2E_POD=api-5f6b7-k2m4p ZEABUR_E2E_LOG_LINE="GET /products" ZEABUR_E2E_FAILING=worker
// ZEABUR_E2E_SECRET=db-credentials ZEABUR_E2E_SECRET_VALUE=correct-horse-battery-staple
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const pod = env("ZEABUR_E2E_POD")
const healthy = env("ZEABUR_E2E_SERVICE", "talker")
const logLine = env("ZEABUR_E2E_LOG_LINE", "zeabur-e2e-line")
const failing = env("ZEABUR_E2E_FAILING", "crasher")
const secret = env("ZEABUR_E2E_SECRET", "talker-secret")
const secretValue = env("ZEABUR_E2E_SECRET_VALUE", "zeabur-e2e-secret")
const project = `/projects/${encodeURIComponent(namespace)}`

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
  if (!pod) throw new Error("ZEABUR_E2E_POD is not set")
})

const logsPod = (page: Page) =>
  page.getByRole("tabpanel", { name: "Logs" }).getByRole("button", { name: /^Replica / })

test("Home names the failing service across projects and lists the project", async ({ page }) => {
  await page.goto("/")
  await expect(page).toHaveURL((url) => url.pathname === "/")
  const nav = page.getByRole("navigation", { name: "Main" })
  await expect(nav.getByRole("link", { name: "Home" })).toHaveAttribute("aria-current", "page")
  const status = page.getByRole("region", { name: "Status" })
  await expect(status).toContainText(/Failing\s*[1-9]\d*/)
  await expect(status).toContainText(/Services\s*\d+\/\d+/)
  await expect(status.getByRole("link")).toHaveAttribute("href", "/cluster")

  const attention = page.getByRole("region", { name: "Needs attention" })
  const row = attention.getByRole("listitem").filter({ hasText: `${namespace}/${failing}` })
  // The kubelet reports `CrashLoopBackOff`, `Error`, or briefly "Running" with its last exit.
  await expect(row).toContainText("Keeps crashing")
  await expect(row).toContainText(/CrashLoopBackOff|Error|last exit 1/)
  await expect(row).toContainText("Deployment")
  if (!namespace.startsWith("kube-")) {
    // Failing projects sort first.
    const projects = page.getByRole("list", { name: "Projects" })
    await expect(
      projects.getByRole("link", { name: new RegExp(`^${namespace}\\b`) }).first(),
    ).toBeVisible()
    const cards = await projects.getByRole("listitem").allInnerTexts()
    const mine = cards.findIndex((text) => text.includes(namespace))
    for (const text of cards.slice(0, mine)) {
      const [healthy, total] = (text.match(/(\d+)\/(\d+) healthy/) ?? []).slice(1).map(Number)
      expect(healthy).toBeLessThan(total ?? 0)
    }
  }

  await row.getByRole("link", { name: `Logs of ${failing}` }).click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${failing}\\?.*tab=logs`))
  await expect(page.getByRole("tab", { name: "Logs" })).toHaveAttribute("aria-selected", "true")

  await page.goto("/projects")
  if (namespace.startsWith("kube-")) {
    await page.getByRole("switch", { name: "Show system namespaces" }).click()
  }
  await expect(
    page.getByRole("link", { name: new RegExp(`^${namespace}\\b`) }).first(),
  ).toBeVisible()
})

test("a popup pressed before its code arrives opens when it does, by click and by key", async ({
  page,
}) => {
  // Holds back the popups' code (@zeabur/design-system's popup.tsx) until `release`.
  const hold = async () => {
    let release = () => {}
    const held = new Promise<void>((resolve) => (release = resolve))
    await page.unrouteAll()
    await page.route(/\/assets\/popup-parts-[^/]*\.js$/, async (route) => {
      await held
      await route.continue()
    })
    return release
  }
  const language = page.getByRole("button", { name: "Account" })
  const english = page.getByRole("menuitem", { name: "English" })

  let release = await hold()
  await page.goto("/")
  await expect(language).toHaveAttribute("aria-haspopup", "menu")
  await expect(language).toHaveAttribute("aria-expanded", "false")
  await language.click()
  await expect(english).toBeHidden()
  release()
  await expect(english).toBeVisible()
  await expect(language).toHaveAttribute("aria-expanded", "true")
  await page.keyboard.press("Escape")
  await expect(language).toBeFocused()

  release = await hold()
  await page.reload()
  await language.focus()
  await page.keyboard.press("Enter")
  release()
  await expect(english).toBeVisible()
  await page.unrouteAll()
})

test("the palette jumps to a service by keyboard, and Enter while composing does nothing", async ({
  page,
}) => {
  await page.goto("/")
  const palette = page.getByRole("dialog", { name: "Command palette" })
  const search = palette.getByRole("combobox", { name: /^Jump to/ })
  // The shortcut listens once the shell has rendered; a key pressed before that is lost, so
  // press again until the palette opens.
  await expect(async () => {
    if (!(await palette.isVisible())) await page.keyboard.press("ControlOrMeta+k")
    await expect(search).toBeFocused({ timeout: 1_000 })
  }).toPass()
  await search.fill(healthy)
  const option = palette.getByRole("option", { name: new RegExp(`in ${namespace}$`) }).first()
  await expect(option).toBeVisible()
  // An IME's Enter (keyCode 229) confirms the composed text, not the highlighted item.
  await search.evaluate((input) => {
    const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, isComposing: true })
    Object.defineProperty(event, "keyCode", { value: 229 })
    Object.defineProperty(event, "which", { value: 229 })
    input.dispatchEvent(event)
  })
  await expect(palette).toBeVisible()
  await expect(page).toHaveURL((url) => url.pathname === "/")
  while ((await option.getAttribute("data-highlighted")) === null)
    await page.keyboard.press("ArrowDown")
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(new RegExp(`/projects/${namespace}/services/[^/]+/${healthy}$`))
  await expect(palette).toBeHidden()

  await page.keyboard.press("ControlOrMeta+k")
  await expect(search).toBeFocused()
  await page.keyboard.press("Escape")
  await expect(palette).toBeHidden()
})

test("the cluster page shows the cluster itself, and its components on their own tab", async ({
  page,
}) => {
  await page.goto("/cluster")
  await expect(page.getByRole("heading", { level: 1, name: "Cluster" })).toBeVisible()
  await expect(page.getByText(/^v1\.\d+\.\d+/).first()).toBeVisible()
  await expect(page.getByRole("region", { name: "Projects" })).toHaveCount(0)
  const nodes = page.getByRole("region", { name: "Nodes" })
  await expect(
    nodes.getByRole("list", { name: "Nodes" }).getByRole("listitem").first(),
  ).toContainText(/Ready|Not ready|Cordoned|Pressure|Unknown/)
  await expect(nodes.getByRole("img", { name: /^CPU/ }).first()).toBeVisible()
  await expect(page.getByRole("region", { name: "Warnings in the last hour" })).toBeVisible()

  await page.getByRole("tab", { name: "Components" }).click()
  await expect(page).toHaveURL(/\/cluster\/components$/)
  const installed = page.getByRole("region", { name: "Installed" })
  await expect(installed.getByRole("listitem").first()).toBeVisible()
  await expect(page.getByRole("region", { name: "Catalog" })).toBeVisible()
  await page.goBack()
  await expect(page).toHaveURL(/\/cluster$/)

  // Old address redirects.
  await page.goto("/settings/components")
  await expect(page).toHaveURL(/\/cluster\/components$/)
})

test("Overview puts the failing service first, with a link to its failing pod's logs", async ({
  page,
}) => {
  await page.goto(project)
  await expect(page.getByRole("heading", { level: 1, name: namespace })).toBeVisible()
  const attention = page.getByRole("region", { name: "Failing" })
  const row = attention.getByRole("listitem").filter({ hasText: failing })
  await expect(row).toContainText("Keeps crashing")
  await expect(row).toContainText(/CrashLoopBackOff|Error|last exit 1/)
  await expect(row).toContainText(/restart/)
  const logs = row.getByRole("link", { name: /^Logs of / })
  const label = (await logs.getAttribute("aria-label")) ?? ""
  const failingPod = label.replace(/^Logs of /, "")
  expect(failingPod.startsWith(failing)).toBe(true)
  await logs.click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${failing}\\?.*tab=logs`))
  await expect(page).toHaveURL(new RegExp(`pod=${failingPod}`))
  await expect(page.getByRole("tab", { name: "Logs" })).toHaveAttribute("aria-selected", "true")
  await expect(logsPod(page)).toHaveAccessibleName(`Replica ${failingPod}`)

  await page.goto(project)
  const services = page.getByRole("table", { name: "Services" })
  // Wait for the rows to render before reading them all at once.
  await expect(services.getByRole("row").filter({ hasText: failing })).toBeVisible()
  await expect(services.getByRole("row").filter({ hasText: healthy })).toBeVisible()
  const names = await services.getByRole("row").allInnerTexts()
  const failingRow = names.findIndex((text) => text.includes(failing))
  const healthyRow = names.findIndex((text) => text.includes(healthy))
  expect(failingRow).toBeGreaterThan(0)
  expect(failingRow).toBeLessThan(healthyRow)
})

test("Architecture draws the services; the quick look opens the service page and Back restores it", async ({
  page,
}) => {
  await page.goto(`${project}/architecture`)
  const card = page.getByRole("button", { name: new RegExp(`^${healthy}\\b`) })
  await expect(card).toBeVisible()
  await expect(page.getByRole("button", { name: new RegExp(`^${failing}\\b`) })).toBeVisible()
  await card.click()
  await expect(page).toHaveURL(
    new RegExp(`service=Deployment%2F${healthy}|service=Deployment/${healthy}`),
  )
  const quickLook = page.getByRole("complementary", { name: `Quick look: ${healthy}` })
  await expect(quickLook).toBeVisible()
  await quickLook.getByRole("link", { name: "Open service page" }).click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${healthy}$`))
  await expect(page.getByRole("heading", { level: 1, name: healthy })).toBeVisible()
  await page.goBack()
  await expect(page.getByRole("complementary", { name: `Quick look: ${healthy}` })).toBeVisible()
})

test("Services filters, sorts, and leads from a pod to its logs", async ({ page }) => {
  await page.goto(`${project}/services`)
  const table = page.getByRole("table", { name: "Services" })
  await expect(table.getByRole("link", { name: new RegExp(`^${healthy}\\b`) })).toBeVisible()

  await page.getByRole("button", { name: /^Needs attention/ }).click()
  await expect(page).toHaveURL(/status=attention/)
  await expect(table.getByRole("link", { name: new RegExp(`^${failing}\\b`) })).toBeVisible()
  await expect(table.getByRole("link", { name: new RegExp(`^${healthy}\\b`) })).toHaveCount(0)
  await page.getByRole("button", { name: "All", exact: true }).click()

  await page.getByRole("searchbox", { name: "Filter services" }).fill(healthy)
  await expect(page).toHaveURL(new RegExp(`q=${healthy}`))
  await expect(table.getByRole("link", { name: new RegExp(`^${failing}\\b`) })).toHaveCount(0)
  await page.getByRole("searchbox", { name: "Filter services" }).fill("")

  await table.getByRole("button", { name: "Restarts" }).click()
  await expect(page).toHaveURL(/sort=restarts/)
  await expect(table.getByRole("columnheader", { name: "Restarts" })).toHaveAttribute(
    "aria-sort",
    "descending",
  )
  await expect(table.getByRole("row").nth(1)).toContainText(failing)

  await table.getByRole("link", { name: `Logs of ${pod}` }).click()
  await expect(page).toHaveURL(new RegExp(`tab=logs.*pod=${pod}|pod=${pod}.*tab=logs`))
  await expect(logsPod(page)).toHaveAccessibleName(`Replica ${pod}`)
})

test("the service page has its tabs, and the Logs tab follows a pod", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${healthy}`)
  await expect(page.getByRole("heading", { level: 1, name: healthy })).toBeVisible()
  const tabs = page.getByRole("tablist", { name: "Service sections" })
  const names = await tabs.getByRole("tab").allInnerTexts()
  expect(names.slice(0, 7).map((n) => n.trim())).toEqual([
    "Overview",
    "Logs",
    "Variables",
    "Usage",
    "Networking",
    "Storage",
    "Settings",
  ])
  expect(names.map((n) => n.trim())).toContain("Releases")
  await page.getByRole("button", { name: "More actions" }).click()
  await expect(page.getByRole("menuitem")).toHaveText([
    "Open terminal",
    "View YAML",
    "Delete service",
  ])
  await page.keyboard.press("Escape")
  await expect(tabs.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true")
  const overview = page.getByRole("tabpanel", { name: "Overview" })
  await expect(overview.getByRole("region", { name: "Status" })).toContainText("Source")
  await overview.getByRole("button", { name: /^Replicas \d+\/\d+$/ }).click()
  await expect(page.getByRole("dialog")).toContainText(pod)
  await page.keyboard.press("Escape")
  await expect(overview.getByRole("button", { name: "Logs", exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Restart" })).toBeEnabled()
  const sections = page.getByRole("navigation", { name: "Project sections" })
  await expect(sections.getByRole("link", { name: "Projects" })).toHaveAttribute(
    "href",
    "/projects",
  )
  const header = page.locator("main header").first()
  await expect(header.getByText(`Deployment/${healthy}`, { exact: true })).toHaveCount(1)
  await expect(header.getByText(/^Namespace/)).toHaveCount(0)

  await tabs.getByRole("tab", { name: "Logs" }).click()
  await expect(page).toHaveURL(/tab=logs/)
  await logsPod(page).click()
  await page.getByRole("button", { name: new RegExp(`^${pod}`) }).click()
  await expect(logsPod(page)).toHaveAccessibleName(`Replica ${pod}`)
  const follow = page.getByRole("button", { name: "Follow" })
  await follow.click()
  await expect(follow).toHaveAttribute("aria-pressed", "true")
  await expect(page.getByRole("log", { name: "Log lines" })).toContainText(logLine, {
    timeout: 30_000,
  })

  await page.getByRole("button", { name: "More actions" }).click()
  await page.getByRole("menuitem", { name: "View YAML" }).click()
  await expect(page).toHaveURL(/tab=yaml/)
  await expect(tabs.getByRole("tab", { name: "YAML" })).toHaveAttribute("aria-selected", "true")
  await tabs.getByRole("tab", { name: "Networking" }).click()
  await expect(page).toHaveURL(/tab=networking/)
  await page.goBack()
  await expect(page).toHaveURL(/tab=yaml/)
})

const canvasZoom = (page: Page) => page.getByText(/^\d+%$/)

test("each project keeps its own Architecture zoom", async ({ page }) => {
  // kube-system is in every cluster.
  const other = namespace === "kube-system" ? "default" : "kube-system"
  await page.goto(`${project}/architecture`)
  await expect(page.getByRole("button", { name: new RegExp(`^${healthy}\\b`) })).toBeVisible()
  const before = await canvasZoom(page).textContent()
  await page.getByRole("button", { name: "Zoom in" }).click()
  await expect(canvasZoom(page)).not.toHaveText(before ?? "")
  const mine = (await canvasZoom(page).textContent()) ?? ""

  await page.getByRole("button", { name: "Switch project" }).first().click()
  await page.getByRole("button", { name: new RegExp(`^${other}\\b`) }).click()
  await expect(page).toHaveURL(new RegExp(`/projects/${other}$`))
  const sections = page.getByRole("navigation", { name: "Project sections" })
  await sections.getByRole("link", { name: /^Architecture/ }).click()
  await expect(page).toHaveURL(new RegExp(`/projects/${other}/architecture`))
  await expect(canvasZoom(page)).toBeVisible()
  // The zoom buttons sit on the canvas, so a wheel over one reaches it.
  await page.getByRole("button", { name: "Zoom in" }).hover()
  await page.mouse.wheel(0, -600)
  await expect(canvasZoom(page)).not.toHaveText(mine)

  await page.goBack()
  await page.goBack()
  await expect(page).toHaveURL(new RegExp(`${project}/architecture`))
  await expect(canvasZoom(page)).toHaveText(mine)
})

test("a Secret's values appear only when revealed; they and the log filter stay with their service", async ({
  page,
}) => {
  await page.goto(`${project}/services/Deployment/${healthy}?tab=logs`)
  const column = page.getByRole("navigation", { name: "Project sections" })
  await page.getByRole("textbox", { name: "Filter lines" }).fill("zeabur-e2e-nothing-matches")
  await column.getByRole("link", { name: new RegExp(`^${failing}\\b`) }).click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${failing}\\?.*tab=logs`))
  await expect(page.getByRole("textbox", { name: "Filter lines" })).toHaveValue("")

  await page.goto(`${project}/services/Deployment/${healthy}?tab=variables`)
  const panel = page.getByRole("tabpanel", { name: /^Variables/ })
  await expect(panel).not.toContainText(secretValue)
  await panel.getByRole("button", { name: `Show values in ${secret}` }).click()
  await expect(panel).toContainText(secretValue)
  await panel.getByRole("button", { name: `Hide values in ${secret}` }).click()
  await expect(panel).not.toContainText(secretValue)
  await panel.getByRole("button", { name: `Show values in ${secret}` }).click()
  await expect(panel).toContainText(secretValue)
  await column.getByRole("link", { name: new RegExp(`^${failing}\\b`) }).click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${failing}\\?.*tab=variables`))
  await column.getByRole("link", { name: new RegExp(`^${healthy}\\b`) }).click()
  await expect(page).toHaveURL(new RegExp(`/services/Deployment/${healthy}\\?.*tab=variables`))
  await expect(panel).not.toContainText(secretValue)
})

test("the Services search box follows the URL on Back and Forward", async ({ page }) => {
  await page.goto(`${project}/services`)
  const box = page.getByRole("searchbox", { name: "Filter services" })
  const table = page.getByRole("table", { name: "Services" })
  const healthyRow = table.getByRole("link", { name: new RegExp(`^${healthy}\\b`) })
  await box.fill(failing)
  await expect(page).toHaveURL(new RegExp(`q=${failing}`))
  await expect(healthyRow).toHaveCount(0)

  const sections = page.getByRole("navigation", { name: "Project sections" })
  await sections.getByRole("link", { name: "All services" }).click()
  await expect(page).not.toHaveURL(/q=/)
  await expect(box).toHaveValue("")
  await expect(healthyRow).toBeVisible()

  await page.goBack()
  await expect(page).toHaveURL(new RegExp(`q=${failing}`))
  await expect(box).toHaveValue(failing)
  await expect(healthyRow).toHaveCount(0)

  await page.goForward()
  await expect(page).not.toHaveURL(/q=/)
  await expect(box).toHaveValue("")
})

// The writes touch only `editor` and its ConfigMap, so the tests above see the same project.
// They expect a fresh namespace: scripts/e2e-ui.sh recreates it before a run.
const editor = "editor"

test("Objects lists the project's objects and opens one as YAML", async ({ page }) => {
  await page.goto(`${project}/objects`)
  await expect(page.getByRole("heading", { level: 1, name: "Objects" })).toBeVisible()
  const list = page.getByRole("list", { name: "Objects" })
  await list.getByRole("button", { name: /^editor-config/ }).click()
  await expect(page).toHaveURL(/object=core(%2F|\/)v1(%2F|\/)configmaps(%2F|\/)editor-config/)
  const panel = page.getByRole("dialog").filter({ hasText: "editor-config" })
  await expect(panel.getByRole("figure", { name: "YAML of editor-config" })).toContainText(
    /mode: (blue|green)/,
  )
  await page.keyboard.press("Escape")
  await expect(page).not.toHaveURL(/object=/)
})

test("Objects reads unusual kinds on click and on a scan", async ({ page }) => {
  await page.goto(`${project}/objects`)
  const list = page.getByRole("list", { name: "Objects" })
  const filter = page.getByRole("searchbox", { name: "Filter objects" })
  // The list is virtual, so find the kind by name.
  await page.getByRole("button", { name: /^Generated/ }).click()
  await filter.fill("Lease")
  const leases = list.getByRole("button", { name: "Read Lease", exact: true })
  await expect(leases).toBeVisible()
  await leases.click()
  await expect(leases).toBeHidden()
  await filter.fill("")
  // Reads every served kind, CRDs included: slow on a slow runner.
  await page.getByRole("button", { name: "Scan all kinds" }).click()
  await expect(page.getByText("All kinds read")).toBeVisible({ timeout: 30_000 })
  await page.getByRole("button", { name: "Refresh" }).click()
  await expect(list.getByRole("button", { name: /^editor-config/ })).toBeVisible()
})

test("YAML never shows a Secret's values", async ({ page }) => {
  await page.goto(`${project}/objects?object=core/v1/secrets/${secret}`)
  const yaml = page.getByRole("figure", { name: `YAML of ${secret}` })
  await expect(yaml).toContainText("kind: Secret")
  await expect(yaml).toContainText("greeting")
  await expect(yaml).not.toContainText(secretValue)

  await page.goto(`${project}/services/Deployment/${healthy}?tab=yaml`)
  const workload = page.getByRole("figure", { name: `YAML of ${healthy}` })
  await expect(workload).toContainText("kind: Deployment")
  await expect(workload).toContainText(secret)
  await expect(workload).not.toContainText(secretValue)
})

const toast = (page: Page, title: string) =>
  page
    .getByRole("region", { name: /^Notifications/ })
    .getByRole("listitem")
    .filter({ hasText: title })
    .first()

async function confirmPlan(
  page: Page,
  title: string | RegExp,
  action: string | RegExp,
  expectText?: string | RegExp,
) {
  const dialog = page.getByRole("dialog", { name: title })
  await expect(dialog).toBeVisible()
  if (expectText) await expect(dialog).toContainText(expectText)
  await dialog.getByRole("button", { name: action, exact: typeof action === "string" }).click()
  await expect(dialog).toHaveCount(0)
  const applied = toast(page, "Applied")
  await expect(applied).toBeVisible({ timeout: 15_000 })
  return applied
}

async function editorReplicas(page: Page): Promise<number | undefined> {
  const answer = await page.request.get(`/api/v1/projects/${namespace}`)
  const body = (await answer.json()) as {
    services: { kind: string; name: string; replicas?: { desired: number } }[]
  }
  return body.services.find((s) => s.kind === "Deployment" && s.name === editor)?.replicas?.desired
}

async function applyNow(page: Page, change: Record<string, unknown>) {
  // As src/auth/session.ts sends them.
  const headers = { Origin: new URL(page.url()).origin, "zeabur-request": "1" }
  const plan = await page.request.post(`/api/v1/projects/${namespace}/plan`, {
    headers,
    data: { changes: [change] },
  })
  expect(plan.status()).toBe(200)
  const { token } = (await plan.json()) as { token: string }
  const applied = await page.request.post(`/api/v1/projects/${namespace}/apply`, {
    headers,
    data: { token },
  })
  expect(applied.status()).toBe(200)
}

test("a variable is added through a plan", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=variables`)
  const panel = page.getByRole("tabpanel", { name: /^Variables/ })
  await panel.getByRole("button", { name: "Add variable" }).click()
  await panel.getByRole("textbox", { name: "Name" }).fill("E2E_ADDED")
  await panel.getByRole("textbox", { name: "Value" }).fill("added-1")
  await panel.getByRole("button", { name: "Review" }).click()
  await confirmPlan(
    page,
    `Variables for ${editor}`,
    "Save variables",
    `Set E2E_ADDED on ${editor}.`,
  )
  await expect(panel).toContainText("E2E_ADDED", { timeout: 15_000 })
  await expect(panel).toContainText("added-1")
})

test("a ConfigMap value is edited where it lives, its key highlighted", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=variables`)
  const panel = page.getByRole("tabpanel", { name: /^Variables/ })
  await panel.getByRole("button", { name: "Edit in ConfigMap editor-config" }).click()
  const sheet = page.getByRole("dialog").filter({ hasText: "editor-config" })
  await expect(sheet.locator("li[data-highlight]")).toContainText("mode")
  await sheet.getByRole("textbox", { name: "mode", exact: true }).fill("green")
  await sheet.getByRole("button", { name: "Review" }).click()
  await confirmPlan(
    page,
    "ConfigMap editor-config",
    "Save values",
    `${editor} reads editor-config and keeps the old value until restarted.`,
  )
  await expect(panel).toContainText("green", { timeout: 15_000 })
})

test("restart of a single pod is confirmed in full, then reported in a toast", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}`)
  await page.getByRole("button", { name: "Restart" }).click()
  const dialog = page.getByRole("dialog", { name: `Restart ${editor}` })
  await expect(dialog.getByRole("note")).toContainText(
    `${editor} has one replica, so it may be down for a few seconds while it restarts.`,
  )
  await expect(dialog).not.toContainText("restartedAt")
  await dialog.getByRole("button", { name: "Technical details" }).click()
  await expect(dialog).toContainText("restartedAt")
  const applied = await confirmPlan(page, `Restart ${editor}`, "Restart")
  await expect(applied).toContainText(`Restart ${editor}`)
  await applied.getByRole("button", { name: "View" }).click()
  await expect(page).toHaveURL(/tab=releases/)
})

test("a change made meanwhile is refused in a toast, and Review again plans it afresh", async ({
  page,
}) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=variables`)
  const panel = page.getByRole("tabpanel", { name: /^Variables/ })
  await panel.getByRole("button", { name: "Add variable" }).click()
  await panel.getByRole("textbox", { name: "Name" }).fill("E2E_REVIEWED")
  await panel.getByRole("textbox", { name: "Value" }).fill("reviewed-1")
  await panel.getByRole("button", { name: "Review" }).click()
  const dialog = page.getByRole("dialog", { name: `Variables for ${editor}` })
  await expect(dialog).toContainText("Set E2E_REVIEWED")
  await applyNow(page, {
    op: "env",
    workload: { kind: "Deployment", name: editor },
    changes: [{ name: "E2E_MEANWHILE", op: "set", value: "x" }],
  })

  await dialog.getByRole("button", { name: "Save variables" }).click()
  await expect(dialog).toHaveCount(0)
  const refused = toast(page, "changed after you reviewed it")
  await expect(refused).toBeVisible({ timeout: 15_000 })
  await refused.getByRole("button", { name: "Review again" }).click()
  await confirmPlan(page, `Variables for ${editor}`, "Save variables", "Set E2E_REVIEWED")
  await expect(panel).toContainText("E2E_REVIEWED", { timeout: 15_000 })
})

test("scaling goes through a plan from Settings, and is undone from its toast", async ({
  page,
}) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=settings`)
  const panel = page.getByRole("tabpanel", { name: "Settings" })
  const replicas = panel.getByRole("textbox", { name: /^Replicas/ })
  const before = await editorReplicas(page)
  if (before === undefined) throw new Error(`${editor} reports no replicas`)
  const more = before + 1
  await replicas.fill(String(more))
  await replicas.press("Enter")
  const applied = await confirmPlan(
    page,
    `Compute for ${editor}`,
    `Scale to ${more}`,
    `Scale ${editor} from ${before} to ${more} replicas.`,
  )
  await expect.poll(() => editorReplicas(page), { timeout: 15_000 }).toBe(more)
  await applied.getByRole("button", { name: "Undo" }).click()
  await expect(toast(page, "Undone")).toBeVisible({ timeout: 15_000 })
  await expect.poll(() => editorReplicas(page), { timeout: 15_000 }).toBe(before)
})

test("a readiness probe is set through a plan", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=settings`)
  const form = page
    .getByRole("tabpanel", { name: "Settings" })
    .locator("form")
    .filter({ has: page.getByRole("heading", { name: "Traffic check" }) })
  const health = page.getByRole("button", { name: "Health checks" })
  if ((await health.getAttribute("aria-expanded")) !== "true") await health.click()
  // The Select's list is portalled to the page's root, outside the form.
  await form.getByRole("combobox", { name: "Check" }).click()
  await page.getByRole("option", { name: "Command" }).click()
  await form.getByRole("textbox", { name: "Command" }).fill("true")
  await form.getByRole("button", { name: "Review" }).click()
  await confirmPlan(
    page,
    `Health checks for ${editor}`,
    "Save health checks",
    "A replica gets no traffic until running true succeeds (checked every 10 s).",
  )
})

test("a volume is added and its claim edited on the Storage tab", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=storage`)
  const panel = page.getByRole("tabpanel", { name: /^Storage/ })
  const add = panel.locator("form").filter({ hasText: "Add a volume" })
  await add.getByRole("textbox", { name: /^Name/ }).fill("e2e-data")
  await add.getByRole("textbox", { name: /^Mount path/ }).fill("/e2e-data")
  await add.getByRole("button", { name: "Review" }).click()
  await confirmPlan(
    page,
    "Add a volume at /e2e-data",
    "Add volume",
    new RegExp(`${editor} gets a new \\S+ volume\\.`),
  )
  const card = panel.locator("form").filter({ has: page.getByRole("button", { name: "Grow" }) })
  await expect(card).toBeVisible({ timeout: 15_000 })
  await expect(panel.getByRole("textbox", { name: /^Mount path/ }).first()).toHaveValue("/e2e-data")
})

test("Releases lists the templates and rolls back to one through a plan", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=releases`)
  // Guarantees a second template to roll back to.
  await applyNow(page, { op: "restart", workload: { kind: "Deployment", name: editor } })
  const panel = page.getByRole("tabpanel", { name: "Releases" })
  const list = panel.getByRole("list", { name: "Releases" })
  // The Deployment controller writes the new ReplicaSet a moment after the apply.
  await expect(async () => {
    await page.reload()
    await expect(list.getByRole("listitem").first()).toBeVisible()
    expect(await list.getByRole("listitem").count()).toBeGreaterThan(1)
  }).toPass({ timeout: 15_000 })
  await expect(list).toContainText("Running")
  await expect(panel.getByRole("heading", { name: /^#\d+ → #\d+$/ })).toBeVisible()
  await list.getByRole("button", { name: "Roll back" }).first().click()
  await confirmPlan(
    page,
    new RegExp(`^Roll back ${editor} to #\\d+$`),
    /^Roll back to revision \d+$/,
    "A rollback leaves ConfigMap and Secret contents as they are.",
  )
})

// CodeMirror and Sonner insert <style> elements at run time, which the CSP must allow; the
// fixture fails on a reported violation.
test("an object's YAML is edited and planned", async ({ page }) => {
  await page.goto(`${project}/objects?object=core/v1/configmaps/editor-config`)
  const panel = page.getByRole("dialog").filter({ hasText: "editor-config" })
  await panel.getByRole("button", { name: "Edit" }).click()
  const text = panel.getByRole("textbox", { name: "YAML of editor-config" })
  await expect(panel.locator(".cm-editor")).toHaveCSS("display", "flex")
  await text.getByText("untouched").dblclick()
  await page.keyboard.type("edited")
  await panel.getByRole("button", { name: "Review" }).click()
  await confirmPlan(page, "Edit editor-config", "Save YAML", "Zeabur writes the YAML as it is")
  await expect(page.locator("[data-sonner-toaster]")).toHaveCSS("position", "fixed")
  await expect(panel.getByRole("figure", { name: "YAML of editor-config" })).toContainText(
    "other: edited",
    { timeout: 15_000 },
  )
})

test("a crash-looping service says why, with its fix and evidence", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${failing}`)
  const diagnosis = page.getByRole("region", { name: "What's wrong" })
  await expect(diagnosis).toContainText(`${failing} keeps crashing and restarting.`, {
    timeout: 60_000,
  })
  await expect(diagnosis).toContainText(/It has restarted \d+ times/)
  await expect(diagnosis).toContainText("The error is in the logs of the run before the restart.")
  await diagnosis
    .getByRole("button", { name: /Evidence/ })
    .first()
    .click()
  await expect(diagnosis).toContainText(/Pod\//)
  await diagnosis.getByRole("button", { name: "Details for Kubernetes users" }).first().click()
  await expect(diagnosis).toContainText("Exit code")
  await diagnosis.getByRole("button", { name: "Open logs" }).first().click()
  await expect(page).toHaveURL(/tab=logs/)
  await expect(logsPod(page)).toHaveAccessibleName(new RegExp(`^Replica ${failing}`))
})

test("Overview lists what the project misses, and a fix is planned and applied", async ({
  page,
}) => {
  await page.goto(project)
  const missing = page.getByRole("region", { name: "Missing setup" })
  const limits = missing
    .getByRole("listitem")
    .filter({ hasText: `${editor} sets no CPU or memory` })
  await expect(limits).toBeVisible({ timeout: 30_000 })
  await expect(limits.getByRole("link", { name: /Resource Management/ })).toHaveAttribute(
    "href",
    /^https:\/\/kubernetes\.io\//,
  )
  await limits.getByRole("button", { name: "Fix" }).click()
  await confirmPlan(
    page,
    `Set CPU and memory for ${editor}`,
    "Save resources",
    /CPU request \S+, Memory request \S+, Memory limit \S+\./,
  )
  await expect(limits).toHaveCount(0, { timeout: 15_000 })
})

test("install texts come in four forms for a catalogue component", async ({ page }) => {
  await page.goto("/cluster/components?component=certificates/cert-manager")
  const sheet = page.getByRole("dialog", { name: /cert-manager/ })
  // The texts are tested server-side (crates/zeabur-catalog/tests/golden).
  const text = (form: string) => sheet.getByRole("figure", { name: new RegExp(` ${form}$`) })
  await expect(text("Helm")).toContainText("cert-manager")
  await sheet.getByRole("tab", { name: "Flux" }).click()
  await expect(text("Flux")).toContainText("cert-manager")
  await sheet.getByRole("button", { name: /Several nodes/ }).click()
  await expect(page).toHaveURL(/profile=multi-node/)
  await sheet.getByRole("tab", { name: "Argo CD" }).click()
  await expect(text("Argo CD")).toContainText("cert-manager")
})

test("Networking shows who may reach a service, and a ready-made policy isolates it", async ({
  page,
}) => {
  await page.goto(`${project}/services/Deployment/${editor}?tab=networking`)
  const panel = page.getByRole("tabpanel", { name: /^Networking/ })
  await expect(panel.getByRole("heading", { name: /^Connection rules/ })).toBeVisible()
  await expect(panel).toContainText("NetworkPolicy")
  await expect(panel).toContainText("network plugin (CNI)")
  await panel.getByRole("button", { name: "Add rule" }).click()
  await page.getByRole("menuitem", { name: "Isolate this service" }).click()
  await confirmPlan(
    page,
    `Isolate ${editor}`,
    "Isolate",
    `${editor} then accepts and sends only what this and other policies allow`,
  )
  await expect(panel).toContainText(`zeabur-isolate-${editor}`, { timeout: 15_000 })
  await expect(panel).toContainText("Blocked")
})

test("a ready-made policy is undone from its toast, and deleted from Objects once its name is typed", async ({
  page,
}) => {
  const policy = `zeabur-allow-ingress-${editor}`
  const allowIngress = async () => {
    await page.goto(`${project}/services/Deployment/${editor}?tab=networking`)
    const panel = page.getByRole("tabpanel", { name: /^Networking/ })
    await panel.getByRole("button", { name: "Add rule" }).click()
    await page.getByRole("menuitem", { name: "Allow traffic from public domains" }).click()
    const applied = await confirmPlan(page, `Let public domains reach ${editor}`, "Allow")
    await expect(panel).toContainText(policy, { timeout: 15_000 })
    return { panel, applied }
  }

  const { panel, applied } = await allowIngress()
  await applied.getByRole("button", { name: "Undo" }).click()
  await expect(toast(page, "Undone")).toBeVisible({ timeout: 15_000 })
  await expect(panel).not.toContainText(policy, { timeout: 15_000 })

  await allowIngress()
  const address = `networking.k8s.io/v1/networkpolicies/${policy}`
  await page.goto(`${project}/objects?object=${encodeURIComponent(address)}`)
  await page.getByRole("button", { name: "Delete", exact: true }).click()
  const dialog = page.getByRole("dialog", { name: `Delete NetworkPolicy ${policy}` })
  await expect(dialog).toContainText(`Delete ${policy}. This can't be undone.`)
  const confirm = dialog.getByRole("button", { name: "Delete", exact: true })
  await expect(confirm).toBeDisabled()
  await dialog.getByRole("textbox", { name: `Type ${policy} to confirm` }).fill(policy)
  await confirm.click()
  await expect(toast(page, "Applied")).toBeVisible({ timeout: 15_000 })
  const gone = await page.request.get(
    `/api/v1/projects/${namespace}/objects/networking.k8s.io/v1/networkpolicies/${policy}`,
  )
  expect(gone.status()).toBe(404)
})

test("a namespace is adopted, named, and let go, through plans", async ({ page }) => {
  // Set by scripts/e2e-ui.sh; the demo server has no such namespace.
  const other = env("ZEABUR_E2E_ADOPT")
  test.skip(!other, "ZEABUR_E2E_ADOPT is not set")
  await page.goto("/projects")
  const row = page.getByRole("listitem").filter({ hasText: other })
  await row.getByRole("button", { name: "Adopt" }).click()
  await confirmPlan(page, `Adopt ${other}`, "Adopt", "Nothing in it changes.")
  await expect(row.getByRole("button", { name: "Adopt" })).toHaveCount(0, { timeout: 15_000 })

  await page.goto(`/projects/${other}/settings`)
  await page.getByRole("textbox", { name: "Project name" }).fill("Adopted in a test")
  await page.getByRole("button", { name: "Change icon" }).click()
  await page.getByRole("button", { name: "Rocket" }).click()
  await page.keyboard.press("Escape")
  await page.getByRole("button", { name: "Save details" }).click()
  await confirmPlan(page, `${other} details`, "Save details")
  await page.goto(`/projects/${other}`)
  await expect(page.getByRole("navigation", { name: "Project sections" })).toContainText(
    "Adopted in a test",
    { timeout: 15_000 },
  )

  await page.goto(`/projects/${other}/settings`)
  await page.getByRole("button", { name: "Remove", exact: true }).click()
  await confirmPlan(
    page,
    `Stop showing ${other} as a project`,
    "Stop showing as a project",
    "Nothing in it is deleted.",
  )
  await page.goto("/projects")
  await expect(
    page.getByRole("listitem").filter({ hasText: other }).getByRole("button", { name: "Adopt" }),
  ).toBeVisible({ timeout: 15_000 })
})
