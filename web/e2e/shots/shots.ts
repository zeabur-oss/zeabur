import type { Page } from "@playwright/test"
import { test } from "../fixtures.ts"
import {
  readManifest,
  serverKey,
  serversOf,
  viewports,
  type Locale,
  type Step,
  type Theme,
} from "./manifest.ts"

const manifest = process.env.ZEABUR_SHOTS_MANIFEST
const out = process.env.ZEABUR_SHOTS_OUT
if (!manifest || !out) throw new Error("ZEABUR_SHOTS_MANIFEST and ZEABUR_SHOTS_OUT must be set")
const server = process.env.ZEABUR_SHOTS_SERVER ?? serverKey({ app: "zeabur", env: {} })
// A fresh shot's server serves one locale and theme.
const only = (JSON.parse(server) as { project?: string }).project

async function run(page: Page, step: Step) {
  if ("click" in step) await page.locator(step.click).first().click()
  else if ("fill" in step) await page.locator(step.fill).first().fill(step.value)
  else if ("hover" in step) await page.locator(step.hover).first().hover()
  else if ("wait" in step) await page.locator(step.wait).first().waitFor()
  else await page.keyboard.press(step.press)
}

// Content must not touch the edge of the image. Take the element alone, then put it on its
// background with space added on each side where its own padding is under 24 px.
async function cropped(page: Page, selector: string, path: string) {
  const element = page.locator(selector).first()
  const frame = await element.evaluate((el) => {
    const s = getComputedStyle(el)
    const pad = (side: string) =>
      Math.max(0, 24 - parseFloat(s.getPropertyValue(`padding-${side}`)))
    let bg = "transparent"
    for (let e: Element | null = el; e && bg === "transparent"; e = e.parentElement) {
      const c = getComputedStyle(e).backgroundColor
      if (c !== "rgba(0, 0, 0, 0)") bg = c
    }
    return {
      width: el.getBoundingClientRect().width,
      padding: ["top", "right", "bottom", "left"].map(pad),
      bg,
      radius: s.borderRadius,
    }
  })
  const options = { animations: "disabled", caret: "hide" } as const
  if (frame.padding.every((p) => p === 0) && parseFloat(frame.radius) === 0)
    return void (await element.screenshot({ ...options, path }))
  const image = (await element.screenshot(options)).toString("base64")
  const sheet = await page.context().newPage()
  await sheet.setContent(
    `<body style="margin:0"><div style="display:inline-block;background:${frame.bg};` +
      `padding:${frame.padding.map((p) => `${p}px`).join(" ")}">` +
      `<img style="display:block;width:${frame.width}px;border-radius:${frame.radius}" src="data:image/png;base64,${image}"></div>`,
  )
  await sheet.locator("div").screenshot({ path })
  await sheet.close()
}

for (const shot of readManifest(manifest).filter((s) => serversOf(s).includes(server))) {
  test.describe(() => {
    test.use({ viewport: viewports[shot.viewport] })

    test(shot.id, async ({ page }, info) => {
      const [locale, theme] = info.project.name.split("/") as [Locale, Theme]
      test.skip(
        !shot.locales.includes(locale) ||
          !shot.themes.includes(theme) ||
          (only !== undefined && only !== info.project.name),
      )
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
      if (shot.clip) await cropped(page, shot.clip, path)
      else await page.screenshot(options)
    })
  })
}
