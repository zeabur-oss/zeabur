import type { Page } from "@playwright/test"
import { test } from "../fixtures.ts"
import { envKey, readManifest, viewports, type Step } from "./manifest.ts"

const manifest = process.env.ZEABUR_SHOTS_MANIFEST
const out = process.env.ZEABUR_SHOTS_OUT
if (!manifest || !out) throw new Error("ZEABUR_SHOTS_MANIFEST and ZEABUR_SHOTS_OUT must be set")
const env = process.env.ZEABUR_SHOTS_ENV ?? envKey({})

async function run(page: Page, step: Step) {
  if ("click" in step) await page.locator(step.click).first().click()
  else if ("fill" in step) await page.locator(step.fill).first().fill(step.value)
  else if ("hover" in step) await page.locator(step.hover).first().hover()
  else await page.keyboard.press(step.press)
}

for (const shot of readManifest(manifest).filter((s) => envKey(s.env) === env)) {
  test.describe(() => {
    test.use({ viewport: viewports[shot.viewport] })

    test(shot.id, async ({ page }, info) => {
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
      if (shot.clip) await page.locator(shot.clip).first().screenshot(options)
      else await page.screenshot(options)
    })
  })
}
