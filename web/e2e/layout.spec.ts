import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// The page's frame and edges. Runs where ui.spec.ts runs, with its variables, and:
//
//   ZEABUR_E2E_CROWDED  a project with 40 or more services (the demo server: analytics)
//   ZEABUR_E2E_VOLUME   a service of the project with a volume, as Kind/name (the demo: StatefulSet/db)
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const healthy = env("ZEABUR_E2E_SERVICE", "talker")
const crowded = env("ZEABUR_E2E_CROWDED")
const volume = env("ZEABUR_E2E_VOLUME")
const project = `/projects/${encodeURIComponent(namespace)}`

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

const widths = [1440, 390]

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
    ready: (page: Page) => page.getByRole("table", { name: "Services" }),
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

test.describe("in a project with many services", () => {
  test.skip(!crowded, "ZEABUR_E2E_CROWDED is not set")

  for (const width of widths) {
    test(`the document never scrolls, only the sidebar and main do, at ${width}px`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 800 })
      await page.goto(`/projects/${encodeURIComponent(crowded)}?view=list`)
      await expect(page.getByRole("table", { name: "Services" })).toBeVisible()
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

for (const locale of ["en-US", "zh-TW"]) {
  test.describe(`a volume's card in ${locale}`, () => {
    test.skip(!volume, "ZEABUR_E2E_VOLUME is not set")
    test.use({ locale, viewport: { width: 400, height: 900 } })

    test("lines its fields up and saves them with one button", async ({ page }) => {
      await page.goto(`${project}/services/${volume}?tab=settings#storage`)
      const card = page
        .locator("form")
        .filter({ has: page.getByRole("switch") })
        .first()
      await card.scrollIntoViewIfNeeded()
      const [size, path] = await card.getByRole("textbox").all()
      const box = async (locator: typeof card) => {
        const b = await locator.boundingBox()
        return b && [Math.round(b.x), Math.round(b.x + b.width)]
      }
      const sizeEdges = await box(size!)
      expect(await box(path!)).toEqual(sizeEdges)
      const toggle = await card.getByRole("switch").boundingBox()
      expect(Math.round(toggle!.x + toggle!.width)).toBe(sizeEdges![1])

      const save = card.getByRole("button")
      await expect(save).toHaveCount(1)
      await expect(save).toBeDisabled()
      await size!.fill("50Gi")
      await expect(save).toBeEnabled()
      await card.getByRole("switch").click()
      await expect(save).toBeEnabled()
      await card.screenshot({ path: test.info().outputPath(`storage-card-${locale}.png`) })
    })
  })
}
