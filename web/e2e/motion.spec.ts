import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// Motion as the design system's README ("Motion") times it, read from the browser's own
// animations (`getAnimations()`) frame by frame. Runs where ui.spec.ts runs, with its variables.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const namespace = env("ZEABUR_E2E_NAMESPACE")
const healthy = env("ZEABUR_E2E_SERVICE", "talker")
const failing = env("ZEABUR_E2E_FAILING", "crasher")
const project = `/projects/${encodeURIComponent(namespace)}`

const curve = "cubic-bezier(0.22, 1, 0.36, 1)"

test.beforeAll(() => {
  if (!namespace) throw new Error("ZEABUR_E2E_NAMESPACE is not set")
})

interface Frame {
  present: boolean
  top: number
  height: number
  opacity: number
  animations: { property: string; duration: number; easing: string }[]
}

/** What `selector` looks like on each frame from just before `act` until `ms` after it. */
async function frames(page: Page, selector: string, act: () => Promise<void>, ms = 600) {
  await page.evaluate(
    ({ selector, ms }) => {
      const seen: Frame[] = []
      ;(window as unknown as { motionFrames: Frame[] }).motionFrames = seen
      const start = performance.now()
      const tick = () => {
        const element = document.querySelector(selector)
        const rect = element?.getBoundingClientRect()
        seen.push({
          present: !!element,
          top: rect?.top ?? 0,
          height: rect?.height ?? 0,
          opacity: element ? Number(getComputedStyle(element).opacity) : 0,
          animations: (element?.getAnimations() ?? []).map((animation) => ({
            property:
              animation instanceof CSSTransition
                ? animation.transitionProperty
                : (animation as CSSAnimation).animationName,
            duration: Number(animation.effect?.getTiming().duration),
            easing: String(animation.effect?.getTiming().easing),
          })),
        })
        if (performance.now() - start < ms) requestAnimationFrame(tick)
      }
      requestAnimationFrame(tick)
    },
    { selector, ms },
  )
  await act()
  await page.waitForTimeout(ms + 100)
  return page.evaluate(() => (window as unknown as { motionFrames: Frame[] }).motionFrames)
}

/** Every distinct animation seen, as `property duration easing`. */
const timings = (seen: Frame[]) => [
  ...new Set(
    seen.flatMap((f) => f.animations.map((a) => `${a.property} ${a.duration} ${a.easing}`)),
  ),
]

const properties = (seen: Frame[]) =>
  new Set(seen.flatMap((f) => f.animations.map((a) => a.property)))

const longest = (seen: Frame[]) =>
  Math.max(0, ...seen.flatMap((f) => f.animations.map((a) => a.duration)))

const palette = '[role="dialog"]'
const quickLook = 'aside[aria-label^="Quick look"]'

async function openPaletteOnce(page: Page) {
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
  // Its body loads on first use; afterwards every open shows it at once.
  await page.keyboard.press("ControlOrMeta+k")
  await expect(page.getByRole("combobox", { name: /Jump to/ })).toBeFocused()
  await page.keyboard.press("Escape")
  await expect(page.locator(palette)).toHaveCount(0)
}

async function openQuickLook(page: Page) {
  await page.goto(`${project}?view=map`)
  const card = page.getByRole("button", { name: new RegExp(`^${healthy}\\b`) })
  await expect(card).toBeVisible()
  return () => card.click()
}

test("the palette opens and closes on the dialog's timings, with its body until it is gone", async ({
  page,
}) => {
  await openPaletteOnce(page)
  const opening = await frames(page, palette, () => page.keyboard.press("ControlOrMeta+k"))
  expect(timings(opening)).toEqual(
    expect.arrayContaining([`opacity 250 ${curve}`, `scale 250 ${curve}`]),
  )

  const open = opening.at(-1)!
  const closing = await frames(page, palette, () => page.keyboard.press("Escape"))
  expect(timings(closing)).toEqual(
    expect.arrayContaining([`opacity 150 ${curve}`, `scale 150 ${curve}`]),
  )
  const leaving = closing.filter((f) => f.present && f.opacity < 1)
  expect(leaving.length).toBeGreaterThanOrEqual(5)
  // The results stay inside while it fades: never the empty box of a body already unmounted.
  for (const frame of leaving) expect(frame.height).toBeGreaterThan(open.height * 0.9)
  expect(closing.at(-1)!.present).toBe(false)
})

test("the palette's input stays where it is while the results change", async ({ page }) => {
  await openPaletteOnce(page)
  await page.keyboard.press("ControlOrMeta+k")
  const input = page.getByRole("combobox", { name: /Jump to/ })
  await expect(input).toBeFocused()
  await page.waitForTimeout(300)
  const top = async () => (await page.locator(palette).boundingBox())!.y
  const before = await top()
  for (const text of ["s", "h", "zzz"]) {
    await input.pressSequentially(text)
    await page.waitForTimeout(100)
    expect(await top()).toBe(before)
  }
  await expect(page.getByText("Nothing found.")).toBeVisible()
  expect(await top()).toBe(before)
})

test("the quick look slides in from the edge and back out before it goes", async ({ page }) => {
  const click = await openQuickLook(page)
  const opening = await frames(page, quickLook, click)
  expect(timings(opening)).toEqual(
    expect.arrayContaining([`opacity 350 ${curve}`, `translate 350 ${curve}`]),
  )
  const closing = await frames(page, quickLook, () => page.keyboard.press("Escape"))
  expect(timings(closing)).toEqual(
    expect.arrayContaining([`opacity 250 ${curve}`, `translate 250 ${curve}`]),
  )
  expect(closing.filter((f) => f.present).length).toBeGreaterThanOrEqual(6)
  expect(closing.at(-1)!.present).toBe(false)
})

test("a popover and a disclosure move on the popup and collapse timings", async ({ page }) => {
  await page.goto(`${project}/services/Deployment/${healthy}`)
  const replicas = page.getByRole("button", { name: /^Replicas \d+\/\d+$/ })
  await expect(replicas).toBeVisible()
  const popover = '[data-slot="popover-content"]'
  const opening = await frames(page, popover, () => replicas.click())
  expect(timings(opening)).toEqual(
    expect.arrayContaining([`opacity 250 ${curve}`, `scale 250 ${curve}`]),
  )
  const closing = await frames(page, popover, () => page.keyboard.press("Escape"))
  expect(timings(closing)).toEqual(expect.arrayContaining([`opacity 150 ${curve}`]))
  expect(closing.at(-1)!.present).toBe(false)

  await page.goto(`${project}/services/Deployment/${failing}`)
  const trigger = page.locator('[data-slot="collapsible-trigger"]').first()
  await expect(trigger).toBeVisible()
  const panel = '[data-slot="collapsible-content"]'
  const expanding = await frames(page, panel, () => trigger.click())
  expect(timings(expanding)).toEqual(
    expect.arrayContaining([`height 250 ${curve}`, `opacity 250 ${curve}`]),
  )
})

test("Home does not shift as it loads", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { shifted: number }
    w.shifted = 0
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries())
        w.shifted += (entry as unknown as { value: number }).value
    }).observe({ type: "layout-shift", buffered: true })
  })
  await page.goto("/")
  await expect(page.getByRole("list", { name: "Projects" })).toBeVisible()
  await page.waitForTimeout(1500)
  // The web font swapping in moves a line of text by a pixel or two, and nothing more.
  expect(
    await page.evaluate(() => (window as unknown as { shifted: number }).shifted),
  ).toBeLessThan(0.001)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("the palette only fades, and its close still waits", async ({ page }) => {
    await openPaletteOnce(page)
    const opening = await frames(page, palette, () => page.keyboard.press("ControlOrMeta+k"))
    expect([...properties(opening)]).toEqual(["opacity"])
    expect(longest(opening)).toBeLessThanOrEqual(150)
    const closing = await frames(page, palette, () => page.keyboard.press("Escape"))
    expect([...properties(closing)]).toEqual(["opacity"])
    expect(closing.filter((f) => f.present && f.opacity < 1).length).toBeGreaterThanOrEqual(1)
    expect(closing.at(-1)!.present).toBe(false)
  })

  test("the quick look fades without moving, and a disclosure opens at once", async ({ page }) => {
    const click = await openQuickLook(page)
    const opening = await frames(page, quickLook, click)
    expect([...properties(opening)]).toEqual(["opacity"])
    const closing = await frames(page, quickLook, () => page.keyboard.press("Escape"))
    expect([...properties(closing)]).toEqual(["opacity"])
    expect(closing.at(-1)!.present).toBe(false)

    await page.goto(`${project}/services/Deployment/${failing}`)
    const trigger = page.locator('[data-slot="collapsible-trigger"]').first()
    await expect(trigger).toBeVisible()
    const expanding = await frames(page, '[data-slot="collapsible-content"]', () => trigger.click())
    expect(expanding.at(-1)!.present).toBe(true)
    expect(properties(expanding).size).toBe(0)
  })
})
