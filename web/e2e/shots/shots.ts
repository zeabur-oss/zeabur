import type { Page } from "@playwright/test"
import { test } from "../fixtures.ts"
import {
  readManifest,
  serverKey,
  viewports,
  type Locale,
  type Step,
  type Theme,
} from "./manifest.ts"

const manifest = process.env.ZEABUR_SHOTS_MANIFEST
const out = process.env.ZEABUR_SHOTS_OUT
if (!manifest || !out) throw new Error("ZEABUR_SHOTS_MANIFEST and ZEABUR_SHOTS_OUT must be set")
const server = process.env.ZEABUR_SHOTS_SERVER ?? serverKey({ app: "zeabur", env: {} })

async function run(page: Page, step: Step) {
  if ("click" in step) await page.locator(step.click).first().click()
  else if ("fill" in step) await page.locator(step.fill).first().fill(step.value)
  else if ("hover" in step) await page.locator(step.hover).first().hover()
  else if ("wait" in step) await page.locator(step.wait).first().waitFor()
  else await page.keyboard.press(step.press)
}

// Content must not touch the edge of the image. Add space around the element until it has 24 px
// on each side, its own padding included. Null: the element has enough padding of its own.
async function padded(page: Page, selector: string) {
  return page
    .locator(selector)
    .first()
    .evaluate((el) => {
      const s = getComputedStyle(el)
      const own = Math.min(
        ...[s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft].map(parseFloat),
      )
      const pad = Math.max(0, 24 - own)
      if (pad === 0) return null
      el.scrollIntoView({ block: "center" })
      const r = el.getBoundingClientRect()
      const x = Math.max(0, r.left - pad)
      const y = Math.max(0, r.top - pad)
      return {
        x,
        y,
        width: Math.min(innerWidth, r.right + pad) - x,
        height: Math.min(innerHeight, r.bottom + pad) - y,
      }
    })
}

for (const shot of readManifest(manifest).filter((s) => serverKey(s) === server)) {
  test.describe(() => {
    test.use({ viewport: viewports[shot.viewport] })

    test(shot.id, async ({ page }, info) => {
      const [locale, theme] = info.project.name.split("/") as [Locale, Theme]
      test.skip(!shot.locales.includes(locale) || !shot.themes.includes(theme))
      await page.goto(shot.path, { waitUntil: "networkidle" })
      if (shot.wait) await page.locator(shot.wait).first().waitFor()
      for (const step of shot.steps) await run(page, step)
      // A pointer left over a link or a row shows its hover state.
      const { width, height } = viewports[shot.viewport]
      if (!("hover" in (shot.steps.at(-1) ?? {}))) await page.mouse.move(width - 1, height - 1)
      await page.waitForLoadState("networkidle")
      await page.evaluate(() => document.fonts.ready)
      const path = `${out}/${info.project.name}/${shot.id}.png`
      const options = { path, animations: "disabled", caret: "hide" } as const
      const clip = shot.clip && (await padded(page, shot.clip))
      if (clip) await page.screenshot({ ...options, clip })
      else if (shot.clip) await page.locator(shot.clip).first().screenshot(options)
      else await page.screenshot(options)
    })
  })
}
