import type { Locator, Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// The page's frame and edges. Runs where ui.spec.ts runs, with its variables, and:
//
//   ZEABUR_E2E_CROWDED  a project with 40 or more services (the demo server: analytics)
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const healthy = env("ZEABUR_E2E_SERVICE", "talker")
const crowded = env("ZEABUR_E2E_CROWDED")
const project = `/projects/${encodeURIComponent(namespace)}`

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

const widths = [1440, 390]

/** The services: a table, or on a phone one line each. */
const services = (page: Page) =>
  page.getByRole("table", { name: "Services" }).or(page.getByRole("list", { name: "Services" }))

/** The left and right edges of every page column in <main>, rounded to the pixel. */
const columnEdges = (page: Page) =>
  page.locator('main [data-slot="page-column"]').evaluateAll((columns) =>
    columns
      .map((column) => column.getBoundingClientRect())
      .filter((rect) => rect.width > 0)
      .map((rect) => [Math.round(rect.left), Math.round(rect.right)]),
  )

/** Whether <main> scrolls sideways: something in it is wider than the column. */
const mainOverflows = (page: Page) =>
  page.locator("main").evaluate((main) => main.scrollWidth > main.clientWidth)

const pages = [
  { name: "Home", path: "/", ready: (page: Page) => page.getByRole("heading", { name: "Home" }) },
  {
    name: "the project map",
    path: project,
    ready: (page: Page) => page.getByRole("heading", { level: 1 }),
  },
  {
    name: "the project list",
    path: `${project}?view=list`,
    ready: services,
  },
  {
    name: "a service",
    path: `${project}/services/Deployment/${healthy}`,
    ready: (page: Page) => page.getByRole("tab", { name: "Overview" }),
  },
  {
    name: "a service's Settings",
    path: `${project}/services/Deployment/${healthy}?tab=settings`,
    ready: (page: Page) => page.getByRole("tabpanel", { name: "Settings" }),
  },
  {
    name: "a service's Networking",
    path: `${project}/services/Deployment/${healthy}?tab=networking`,
    ready: (page: Page) => page.getByRole("tabpanel", { name: "Networking" }),
  },
  {
    name: "the project's settings",
    path: `${project}/settings`,
    ready: (page: Page) => page.getByRole("heading", { level: 1 }),
  },
]

for (const width of widths) {
  for (const { name, path, ready } of pages) {
    test(`every block on ${name} shares one column at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 })
      await page.goto(path)
      await expect(ready(page).first()).toBeVisible()
      await expect.poll(() => columnEdges(page).then((edges) => edges.length)).toBeGreaterThan(0)
      const edges = await columnEdges(page)
      const [first] = edges
      for (const edge of edges) expect(edge, `${JSON.stringify(edges)}`).toEqual(first)
      expect(await mainOverflows(page)).toBe(false)
    })
  }
}

for (const width of [390, 1440]) {
  test(`a service's Settings keeps space below its last block at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 })
    await page.goto(`${project}/services/Deployment/${healthy}?tab=settings`)
    const panel = page.getByRole("tabpanel", { name: "Settings" })
    await expect(panel).toBeVisible()
    const space = await page.locator("main").evaluate(
      (main, panel) => {
        main.scrollTop = main.scrollHeight
        return main.getBoundingClientRect().bottom - panel!.getBoundingClientRect().bottom
      },
      await panel.elementHandle(),
    )
    expect(space).toBeGreaterThanOrEqual(48)
  })
}

for (const width of [1440, 1960]) {
  test(`the quick look sits at the map's right edge, beside its service, at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 })
    await page.goto(`${project}?view=map`)
    const card = page.getByRole("button", { name: new RegExp(`^${healthy}\\b`) })
    await card.click()
    const panel = page.getByRole("complementary", { name: `Quick look: ${healthy}` })
    await expect(panel).toBeVisible()
    await expect(panel).not.toHaveCSS("translate", /%/)
    const box = async (locator: Locator) => {
      const rect = (await locator.boundingBox())!
      return { left: Math.round(rect.x), right: Math.round(rect.x + rect.width) }
    }
    const sheet = await box(panel)
    const map = await box(page.locator("[data-map-row]"))
    const node = await box(card)
    expect(sheet.right).toBe(map.right)
    expect(map.right).toBe(width)
    expect(sheet.right - sheet.left).toBe(400)
    expect(node.right).toBeLessThanOrEqual(sheet.left)
    expect(node.left).toBeGreaterThanOrEqual(map.left)
  })
}

test("every settings card starts its values at one left edge", async ({ page }) => {
  await page.goto("/settings")
  const cards = page.locator("main section")
  await expect(
    page.getByRole("region", { name: "Usage history" }).locator("dd").first(),
  ).toBeVisible()
  for (const card of await cards.all()) {
    const lefts = await card
      .locator("dd, form")
      .evaluateAll((values) =>
        values.map((value) => Math.round(value.getBoundingClientRect().left)),
      )
    expect(lefts.length).toBeGreaterThan(0)
    expect(new Set(lefts).size, `${JSON.stringify(lefts)}`).toBe(1)
  }
})

test.describe("in a project with many services", () => {
  test.skip(!crowded, "ZEABUR_E2E_CROWDED is not set")

  for (const width of widths) {
    test(`the document never scrolls, only the sidebar and main do, at ${width}px`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 800 })
      await page.goto(`/projects/${encodeURIComponent(crowded)}?view=list`)
      await expect(services(page)).toBeVisible()
      const documentHeight = () =>
        page.evaluate(() => [document.scrollingElement!.scrollHeight, window.innerHeight])
      const [height, inner] = await documentHeight()
      expect(height).toBe(inner)
      expect(await mainOverflows(page)).toBe(false)

      const main = page.locator("main")
      await main.hover()
      await page.mouse.wheel(0, 2000)
      await expect.poll(() => main.evaluate((m) => m.scrollTop)).toBeGreaterThan(0)
      expect(await page.evaluate(() => window.scrollY)).toBe(0)

      if (width < 768) return
      const sidebar = page.getByRole("navigation", { name: "Project sections" })
      const list = sidebar.locator('[data-slot="sidebar-content"]')
      await list.hover()
      await page.mouse.wheel(0, 2000)
      await expect.poll(() => list.evaluate((l) => l.scrollTop)).toBeGreaterThan(0)
      expect(await page.evaluate(() => window.scrollY)).toBe(0)
      // Its header and footer stay where they were.
      await expect(sidebar.getByRole("link", { name: "Projects" })).toBeInViewport()
      expect(await documentHeight()).toEqual([inner, inner])
    })
  }
})

/** Enabled or not, every primary button on the page that is disabled. */
const disabledPrimaries = (page: Page) =>
  page.locator(
    "main button.button-primary:disabled, main button.button-primary[aria-disabled=true]",
  )

for (const width of [1440, 390, 1960]) {
  test.describe(`Storage at ${width}px`, () => {
    test.use({ viewport: { width, height: 900 } })

    test("the volume list and its add row span the column, with no disabled primary button", async ({
      page,
    }) => {
      await page.goto(`${project}/services/Deployment/${healthy}?tab=settings#storage`)
      const part = page.locator("#storage")
      await expect(part.getByRole("heading", { name: "Volumes" })).toBeVisible()
      await expect(disabledPrimaries(page)).toHaveCount(0)

      const edit = part.getByRole("button", { name: "Edit", exact: true })
      if (await edit.isVisible()) await edit.click()
      else await part.getByRole("button", { name: "Add a volume" }).click()
      const list = part.getByRole("group", { name: "Volumes" })
      const add = list.getByRole("button", { name: "Add a volume" })
      if (!(await list.getByRole("textbox", { name: "Mount path of the new volume" }).count()))
        await add.click()
      const row = add.locator("..")
      const edges = (locator: Locator) =>
        locator.evaluate((element) => {
          const rect = element.getBoundingClientRect()
          return [Math.round(rect.left), Math.round(rect.right)]
        })
      const column = await edges(part)
      expect(await edges(list)).toEqual(column)
      const [left, right] = column as [number, number]
      const [rowLeft, rowRight] = (await edges(row)) as [number, number]
      // The add row sits inside the list's border, one pixel in on each side.
      expect(rowLeft - left).toBeLessThanOrEqual(1)
      expect(right - rowRight).toBeLessThanOrEqual(1)
      expect(await mainOverflows(page)).toBe(false)
      await expect(disabledPrimaries(page)).toHaveCount(0)
      await part.screenshot({ path: test.info().outputPath(`storage-${width}.png`) })
    })
  })
}

test("the account card's lines share one left edge", async ({ page }) => {
  await page.route("**/api/v1/session", (route) => route.fulfill({ json: { signsIn: true } }))
  await page.route("**/api/v1/whoami", (route) =>
    route.fulfill({
      json: {
        username: "zeabur:ada.lovelace.with.a.rather.long.address@example.com",
        groups: ["system:authenticated"],
      },
    }),
  )
  await page.route("**/api/v1/cluster", (route) =>
    route.fulfill({
      json: {
        name: "zeabur-l2",
        nameSource: "install",
        kubernetesVersion: "v1.36.5+k3s1",
      },
    }),
  )
  await page.setViewportSize({ width: 1440, height: 900 })
  await page.goto("/")
  const card = page
    .getByRole("navigation", { name: "Main" })
    .locator('button:has([data-slot="avatar"])')
  await expect(card).toContainText("zeabur-l2")
  await expect(card.getByTitle(/Name from the installation/)).toContainText("zeabur-l2")
  await expect(card).toContainText("ada.lovelace")
  await expect(card).not.toContainText("zeabur:")
  await expect(card.locator('[data-slot="avatar"]')).toContainText("AD")
  const lefts = await card.evaluate((chip) => {
    const column = chip.querySelector(":scope > span.flex-col")
    return [...(column?.children ?? [])].map((line) =>
      Math.round(line.getBoundingClientRect().left),
    )
  })
  expect(lefts.length).toBeGreaterThan(1)
  expect(new Set(lefts).size, JSON.stringify(lefts)).toBe(1)
  const widths = await card.evaluate((chip) =>
    [...(chip.querySelector(":scope > span.flex-col")?.children ?? [])].map(
      (line) => line.getBoundingClientRect().width,
    ),
  )
  for (const width of widths) expect(width).toBeGreaterThan(40)
})
