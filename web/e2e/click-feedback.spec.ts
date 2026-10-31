import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// A click answers before its data or code arrives. Runs where ui.spec.ts runs, with its variables.
// With ZEABUR_E2E_SHOTS set, the last test saves what the waiting states look like there.
const namespace = process.env.ZEABUR_E2E_NAMESPACE ?? ""
const editor = "editor"
const project = `/projects/${encodeURIComponent(namespace)}`
const shots = process.env.ZEABUR_E2E_SHOTS

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

/** Holds the requests `match` selects until `release`; none is answered before. */
async function hold(page: Page, match: (url: URL) => boolean) {
  let release!: () => void
  const released = new Promise<void>((resolve) => (release = resolve))
  await page.route(match, async (route) => {
    await released
    await route.continue().catch(() => undefined)
  })
  return release
}

// The project page waits for its events, which Home never reads. Home can already hold the
// project and its namespace: a problem row reads the namespace to offer its logs.
const projectEvents = (url: URL) => url.pathname === `/api/v1/projects/${namespace}/events`
const planChunk = (url: URL) => /\/assets\/plan-dialog-[^/]*\.js$/.test(url.pathname)

// The router swaps the old page for the route's skeleton after its pending delay (1 s), so what
// stood under the bar is read when the bar appears, not by a later assertion.
async function navigateWhileHeld(page: Page) {
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  const release = await hold(page, projectEvents)
  await page.evaluate(() => {
    const w = window as unknown as { underBar?: string | null }
    const observer = new MutationObserver(() => {
      if (!document.querySelector('[role="progressbar"]')) return
      w.underBar = document.querySelector("main h1")?.textContent ?? null
      observer.disconnect()
    })
    observer.observe(document.body, { childList: true, subtree: true })
  })
  await page.getByRole("link", { name: namespace }).first().click()
  return release
}

const underBar = (page: Page) =>
  page.evaluate(() => (window as unknown as { underBar?: string | null }).underBar)

async function saveWhileHeld(page: Page) {
  const release = await hold(page, planChunk)
  await page.goto(`${project}/services/Deployment/${editor}?tab=variables`)
  const panel = page.getByRole("tabpanel", { name: "Variables" })
  await panel.getByRole("button", { name: "Edit", exact: true }).click()
  await panel.getByRole("textbox", { name: "Value of NOTE" }).fill("click-feedback")
  await page.getByRole("button", { name: "Save", exact: true }).click()
  return { release, panel }
}

test("a navigation shows progress before the page's data arrives, and keeps the old page", async ({
  page,
}) => {
  const release = await navigateWhileHeld(page)
  await expect(page.getByRole("progressbar", { name: "Opening the page…" })).toBeVisible()
  expect(await underBar(page)).toBe("Home")
  release()
  await expect(page).toHaveURL((url) => url.pathname.startsWith(project))
  await expect(page.getByRole("progressbar")).toHaveCount(0)
})

test("the skeleton that replaces the old page shows at once", async ({ page }) => {
  const release = await navigateWhileHeld(page)
  await page.evaluate(() => {
    const w = window as unknown as { skeleton?: number }
    const observer = new MutationObserver(() => {
      const row = document.querySelector("main output > div")
      if (!row) return
      observer.disconnect()
      // Before its own 300 ms delay ends, a skeleton that waited for it is still transparent.
      setTimeout(() => (w.skeleton = Number(getComputedStyle(row).opacity)), 100)
    })
    observer.observe(document.body, { childList: true, subtree: true })
  })
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { skeleton?: number }).skeleton))
    .toBeGreaterThan(0)
  release()
})

test("a navigation the data answers at once shows no progress", async ({ page }) => {
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await page.evaluate(() => {
    const w = window as unknown as { bars: number }
    w.bars = 0
    new MutationObserver(() => {
      if (document.querySelector('[role="progressbar"]')) w.bars++
    }).observe(document.body, { childList: true, subtree: true })
  })
  await page.getByRole("link", { name: namespace }).first().click()
  await expect(page).toHaveURL((url) => url.pathname.startsWith(project))
  expect(await page.evaluate(() => (window as unknown as { bars: number }).bars)).toBe(0)
})

test("Save opens the review at once while its code loads", async ({ page }) => {
  const { release, panel } = await saveWhileHeld(page)
  const dialog = page.getByRole("dialog", { name: `Save changes to ${editor}` })
  await expect(dialog).toBeVisible()
  await expect(dialog.getByRole("status")).toContainText("Checking changes…")
  await expect(dialog.getByRole("button", { name: "Save variables" })).toHaveCount(0)
  release()
  await expect(dialog.getByRole("button", { name: "Save variables" })).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel" }).click()
  await expect(dialog).toBeHidden()
  await expect(panel).toBeVisible()
})

for (const scheme of ["light", "dark"] as const) {
  for (const width of [390, 1440, 1960]) {
    test(`saves the waiting states as screenshots at ${width} px, ${scheme}`, async ({ page }) => {
      test.skip(!shots, "ZEABUR_E2E_SHOTS is not set")
      await page.emulateMedia({ colorScheme: scheme })
      await page.setViewportSize({ width, height: 900 })
      const release = await navigateWhileHeld(page)
      await expect(page.getByRole("progressbar")).toBeVisible()
      await page.screenshot({ path: `${shots}/progress-${width}-${scheme}.png` })
      release()
      await expect(page).toHaveURL((url) => url.pathname.startsWith(project))
      await page.unrouteAll({ behavior: "wait" })

      const saved = await saveWhileHeld(page)
      const dialog = page.getByRole("dialog", { name: `Save changes to ${editor}` })
      await expect(dialog.getByRole("status")).toContainText("Checking changes…")
      await page.screenshot({ path: `${shots}/dialog-${width}-${scheme}.png` })
      saved.release()
    })
  }
}
