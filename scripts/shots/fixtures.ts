// The fixtures of the screenshot specs (sets.ts), run by scripts/docs-shots.sh:
//   ZEABUR_SHOTS_OUT   receives <set>/<locale>/<theme>/<id>.png
//   ZEABUR_SHOTS_DEMO  the demo server (crates/zeabur-api/examples/demo.rs), for app zeabur
//   ZEABUR_DEMO_NOW    the time now, for the demo and the browser
import { spawn } from "node:child_process"
import { once } from "node:events"
import { readFile } from "node:fs/promises"
import { createServer } from "node:http"
import type { AddressInfo } from "node:net"
import { extname, join, resolve } from "node:path"
import { createInterface } from "node:readline"
import type { Page } from "@playwright/test"
import { expect, test as base } from "zeabur-web/e2e/fixtures"
import type { App } from "./sets.ts"

export { expect }

const out = process.env.ZEABUR_SHOTS_OUT
if (!out) throw new Error("ZEABUR_SHOTS_OUT is not set")

type Shot = {
  /** CSS selector to crop to; at least 24 px around it. */
  clip?: string
  /** With `clip`: the crop ends at the bottom of this element. */
  until?: string
  /** Text that changes on every run, such as a new invitation's token, left out of the image. */
  blank?: string
  /** False keeps the window at its size, for the cover's plate. */
  fit?: boolean
  /** Leaves the pointer where the steps left it, to show a hover state. */
  keepPointer?: boolean
}

type Server = { url: string; stop: () => Promise<void> }

export const test = base.extend<
  {
    app: App
    demo: Record<`ZEABUR_DEMO_${string}`, string>
    fresh: boolean
    shot: (options?: Shot) => Promise<void>
  },
  { servers: Map<string, Promise<Server>> }
>({
  app: ["zeabur", { option: true }],
  demo: [{}, { option: true }],
  // The shots of one app and environment share a demo server in each worker; with `fresh` each
  // test, in each locale and theme, has its own, since its steps change what the demo holds.
  fresh: [false, { option: true }],
  servers: [
    // oxlint-disable-next-line no-empty-pattern -- Playwright reads a fixture's dependencies from it
    async ({}, use) => {
      const servers = new Map<string, Promise<Server>>()
      await use(servers)
      await Promise.all([...servers.values()].map(async (server) => (await server).stop()))
    },
    { scope: "worker" },
  ],
  baseURL: async ({ app, demo, fresh, servers }, use) => {
    if (fresh) {
      const own = await start(app, demo)
      await use(own.url)
      return own.stop()
    }
    const key = JSON.stringify({ app, demo: Object.fromEntries(Object.entries(demo).sort()) })
    if (!servers.has(key)) servers.set(key, start(app, demo))
    await use((await servers.get(key)!).url)
  },
  page: async ({ page }, use) => {
    if (process.env.ZEABUR_DEMO_NOW) await page.clock.setFixedTime(process.env.ZEABUR_DEMO_NOW)
    await use(page)
  },
  shot: async ({ page }, use, info) => {
    await use(async ({ clip, until, blank, fit = true, keepPointer = false } = {}) => {
      if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(info.title))
        throw new Error(`${info.title}: the title is the file name and must be kebab-case`)
      if (fit) await fitWindow(page, clip)
      // A pointer left over a link or a row shows its hover state.
      const { width, height } = page.viewportSize()!
      if (!keepPointer) await page.mouse.move(width - 1, height - 1)
      await page.waitForLoadState("networkidle")
      await page.evaluate(() => document.fonts.ready)
      const path = `${out}/${info.project.name}/${info.title}.png`
      if (clip) await cropped(page, clip, until, path, blank)
      else await page.screenshot({ ...options(blank), path, fullPage: fit })
    })
  },
})

// Each server binds port 0 itself, so the system hands it a free port; a port chosen before the
// server starts can be taken by another worker's server in between.
async function start(app: App, demo: Record<string, string>): Promise<Server> {
  return app === "zeabur" ? startDemo(demo) : startDesktop()
}

// The demo prints the address it listens on as its first line.
async function startDemo(demo: Record<string, string>): Promise<Server> {
  const child = spawn(demoServer(), [], {
    env: { ...process.env, ...demo, ZEABUR_DEMO_ADDR: "127.0.0.1:0" },
    stdio: ["ignore", "pipe", "pipe"],
  })
  let log = ""
  child.stderr.on("data", (chunk) => (log += chunk))
  const exited = new Promise((done) => child.once("exit", done))
  const stop = async () => {
    child.kill()
    await exited
  }
  const [line] = await Promise.race([
    once(createInterface({ input: child.stdout }), "line"),
    exited.then(() => [undefined]),
  ])
  const url = /^listening on (http:\/\/\S+)$/.exec(line ?? "")?.[1]
  if (!url) {
    await stop()
    throw new Error(`the zeabur demo server did not start (${JSON.stringify(demo)}):\n${line ?? ""}\n${log}`)
  }
  // The demo's controllers settle its fixtures on their first ticks, such as a build that
  // started days ago finishing, 400 ms after it starts.
  await new Promise((done) => setTimeout(done, 1000))
  return { url, stop }
}

function demoServer(): string {
  const demo = process.env.ZEABUR_SHOTS_DEMO
  if (!demo) throw new Error("ZEABUR_SHOTS_DEMO is not set")
  return demo
}

// The desktop's pages are the files of desktop/web/dist, where desktop/web/scripts/shots-demo.ts
// answers the commands in the page.
const dist = resolve(import.meta.dirname, "../../desktop/web/dist")
const types: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".json": "application/json",
}

async function startDesktop(): Promise<Server> {
  const server = createServer(async (request, response) => {
    const path = new URL(request.url ?? "/", "http://desktop").pathname
    const file = join(dist, path.endsWith("/") ? `${path}index.html` : path)
    try {
      if (!file.startsWith(`${dist}/`)) throw new Error("outside dist")
      const body = await readFile(file)
      response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" })
      response.end(body)
    } catch {
      response.writeHead(404).end()
    }
  })
  server.listen(0, "127.0.0.1")
  await once(server, "listening")
  const { port } = server.address() as AddressInfo
  const stop = () => new Promise<void>((done) => server.close(() => done()))
  return { url: `http://127.0.0.1:${port}`, stop }
}

const options = (blank: string | undefined) =>
  ({
    animations: "disabled",
    caret: "hide",
    ...(blank ? { style: `${blank} { color: transparent !important; }` } : {}),
  }) as const

async function settle(page: Page) {
  await page.waitForLoadState("networkidle")
  await page.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  )
}

// A scroll area as tall as the window (the app's main, a drawer's body), or a page at least as
// tall as the window, cuts its content off at the window's edge or ends in empty space. Make the
// window as tall as the content.
async function fitWindow(page: Page, selector = "html") {
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
    const painted = (e: Element) => {
      const s = getComputedStyle(e)
      const marks = [
        e.childElementCount === 0,
        s.backgroundColor !== "rgba(0, 0, 0, 0)",
        s.backgroundImage !== "none",
        parseFloat(s.borderBottomWidth) > 0,
        s.boxShadow !== "none",
      ]
      return s.visibility !== "hidden" && marks.some(Boolean)
    }
    const top = el.getBoundingClientRect().top - el.scrollTop
    let bottom = 0
    for (const e of el.querySelectorAll("*")) {
      const r = e.getBoundingClientRect()
      if (r.width > 1 && r.height > 1 && painted(e)) bottom = Math.max(bottom, r.bottom - top)
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
async function cropped(
  page: Page,
  selector: string,
  until: string | undefined,
  path: string,
  blank: string | undefined,
) {
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
  const shoot = (path?: string) =>
    end
      ? page.screenshot({ ...options(blank), path, clip: frame.box })
      : element.screenshot({ ...options(blank), path })
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
