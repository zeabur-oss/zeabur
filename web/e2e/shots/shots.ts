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

async function settle(page: Page) {
  await page.waitForLoadState("networkidle")
  await page.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  )
}

// A scroll area as tall as the window (the app's main, a drawer's body), or a page at least as
// tall as the window, cuts its content off at the window's edge or ends in empty space. Make the
// window as tall as the content.
async function fit(page: Page, selector = "html") {
  // A drawer is as tall as the window and scrolls in its body; it then ends at its footer.
  const inner = await page
    .locator(selector)
    .first()
    .evaluate((el) => {
      const scrolls = (e: Element) => /auto|scroll/.test(getComputedStyle(e).overflowY)
      if (el === document.documentElement || scrolls(el)) return false
      const drawer =
        getComputedStyle(el).position === "fixed" &&
        el.getBoundingClientRect().height >= window.innerHeight - 1
      const body = drawer
        ? [...el.querySelectorAll("*")]
            .filter((e) => scrolls(e) && e.clientHeight > 0)
            .sort((a, b) => b.clientHeight - a.clientHeight)[0]
        : undefined
      body?.setAttribute("data-shot-fit", "")
      return body !== undefined
    })
  if (inner === false && !(await isScrollArea(page, selector))) return
  const element = page.locator(inner ? "[data-shot-fit]" : selector).first()
  const { width, height } = page.viewportSize()!
  const chrome = height - (await element.evaluate((el) => el.clientHeight))
  await page.setViewportSize({ width, height: 200 })
  await settle(page)
  // Down to the lowest box that shows something: wrappers and margins below it are left out,
  // since the crop adds its own space. A drawer's body keeps its own bottom padding above the
  // footer.
  const content = await element.evaluate((el, inner) => {
    const top = el.getBoundingClientRect().top - el.scrollTop
    let bottom = 0
    for (const e of el.querySelectorAll("*")) {
      const r = e.getBoundingClientRect()
      if (r.width <= 1 || r.height <= 1) continue
      const s = getComputedStyle(e)
      const painted =
        e.childElementCount === 0 ||
        s.backgroundColor !== "rgba(0, 0, 0, 0)" ||
        s.backgroundImage !== "none" ||
        parseFloat(s.borderBottomWidth) > 0 ||
        s.boxShadow !== "none"
      if (painted && s.visibility !== "hidden") bottom = Math.max(bottom, r.bottom - top)
    }
    const padding = inner ? parseFloat(getComputedStyle(el).paddingBottom) : 0
    return Math.ceil(bottom + padding)
  }, inner)
  const fitted = content + chrome
  await page.setViewportSize({ width, height: inner ? Math.min(fitted, 2400) : fitted })
  await settle(page)
}

async function isScrollArea(page: Page, selector: string) {
  return page
    .locator(selector)
    .first()
    .evaluate(
      (el) => el === document.documentElement || /auto|scroll/.test(getComputedStyle(el).overflowY),
    )
}

// Content must not touch the edge of the image. Take the element alone, then put it on its
// background with space added on each side where its own padding is under 24 px, and its own
// rounded corners, so a dialog's backdrop does not show in them. With `until` the image ends at
// the bottom of that element.
async function cropped(page: Page, selector: string, until: string | undefined, path: string) {
  const element = page.locator(selector).first()
  const end = until ? await page.locator(until).first().boundingBox() : null
  const frame = await element.evaluate((el, end) => {
    const s = getComputedStyle(el)
    const pad = (side: string) =>
      Math.max(
        0,
        24 - (end && side === "bottom" ? 0 : parseFloat(s.getPropertyValue(`padding-${side}`))),
      )
    let bg = "transparent"
    for (let e: Element | null = el; e && bg === "transparent"; e = e.parentElement) {
      const c = getComputedStyle(e).backgroundColor
      if (c !== "rgba(0, 0, 0, 0)") bg = c
    }
    const box = el.getBoundingClientRect()
    return {
      box: {
        x: box.x,
        y: box.y,
        width: box.width,
        height: (end ? end.y + end.height : box.bottom) - box.y,
      },
      padding: ["top", "right", "bottom", "left"].map(pad),
      radius: s.borderRadius,
      bg,
    }
  }, end)
  const options = { animations: "disabled", caret: "hide" } as const
  const shoot = (path?: string) =>
    end
      ? page.screenshot({ ...options, path, clip: frame.box })
      : element.screenshot({ ...options, path })
  if (frame.padding.every((p) => p === 0) && parseFloat(frame.radius) === 0)
    return void (await shoot(path))
  const image = (await shoot()).toString("base64")
  const sheet = await page.context().newPage()
  await sheet.setContent(
    `<body style="margin:0"><div style="display:inline-block;background:${frame.bg};` +
      `padding:${frame.padding.map((p) => `${p}px`).join(" ")}">` +
      `<img style="display:block;width:${frame.box.width}px;border-radius:${frame.radius}" src="data:image/png;base64,${image}"></div>`,
  )
  await sheet.locator("div").screenshot({ path })
  await sheet.close()
}

for (const shot of readManifest(manifest).shots.filter((s) => serversOf(s).includes(server))) {
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
      if (shot.viewport !== "cover") await fit(page, shot.clip)
      // A pointer left over a link or a row shows its hover state.
      const { width, height } = page.viewportSize()!
      if (!("hover" in (shot.steps.at(-1) ?? {}))) await page.mouse.move(width - 1, height - 1)
      await page.waitForLoadState("networkidle")
      await page.evaluate(() => document.fonts.ready)
      const path = `${out}/${info.project.name}/${shot.id}.png`
      const options = { path, animations: "disabled", caret: "hide" } as const
      if (shot.clip) await cropped(page, shot.clip, shot.until, path)
      else await page.screenshot({ ...options, fullPage: shot.viewport !== "cover" })
    })
  })
}
