// The desktop app's screenshots (sets.ts): the pages of desktop/web in a browser, with the Tauri
// commands answered by demo.ts. A window's size is the app's, so the pages are not fitted.
import { execFileSync } from "node:child_process"
import { join } from "node:path"
import type { Page } from "@playwright/test"
import { demo as inPage, pasteSample, type Scenario } from "./demo.ts"
import { expect, test } from "./fixtures.ts"

declare const demo: typeof inPage

// demo.ts as one script for the page; `bun build` prints it.
const script = execFileSync(
  "bun",
  ["build", "--target=browser", "--format=iife", join(import.meta.dirname, "demo.ts")],
  { encoding: "utf8" },
)

// A Mac's user agent: the first-run page names the kubeconfig path by the system.
const mac =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
test.use({ app: "desktop", viewport: { width: 1100, height: 720 }, userAgent: mac })

async function open(page: Page, path: string, scenario: Scenario) {
  await page.addInitScript(`${script};demo.install(${JSON.stringify(scenario)})`)
  await page.goto(path, { waitUntil: "networkidle" })
}

// Every row has the answer of its probe.
async function listed(page: Page) {
  await expect(page.locator("main li")).toHaveCount(7)
  await expect(page.locator('[data-slot="badge"][data-variant="idle"]')).toHaveCount(0)
}

test("desktop-first", async ({ page, shot }) => {
  await open(page, "/", { sources: false })
  await expect(page.getByRole("button").last()).toBeVisible()
  await shot({ fit: false })
})

test("desktop-list", async ({ page, shot }) => {
  await open(page, "/", { sources: true })
  await listed(page)
  await shot({ fit: false })
})

test("desktop-search", async ({ page, shot }) => {
  await open(page, "/", { sources: true })
  await listed(page)
  await page.locator("main header input").fill("staging")
  await expect(page.locator("main li")).toHaveCount(2)
  await shot({ fit: false })
})

test("desktop-paste", async ({ page, shot }) => {
  await open(page, "/", { sources: true })
  await listed(page)
  await page.locator("aside [aria-haspopup=menu]").click()
  await page.getByRole("menuitem").nth(2).click()
  await page.locator("#paste-name").fill("homelab")
  await page.locator("#paste-yaml").fill(pasteSample)
  await expect(page.locator('[data-slot="dialog-content"]')).toBeVisible()
  await shot({ fit: false })
})

test("desktop-drop", async ({ page, shot }) => {
  await open(page, "/", { sources: true })
  await listed(page)
  await page.evaluate(() =>
    demo.emit("tauri://drag-enter", { paths: ["/Users/demo/clusters"], position: { x: 0, y: 0 } }),
  )
  await expect(page.locator(".border-dashed")).toBeVisible()
  await shot({ fit: false })
})

test("desktop-cluster", async ({ page, shot }) => {
  await open(page, "/cluster.html?name=zeabur-demo", {
    sources: true,
    failure: "error sending request for url (http://127.0.0.1:8080/version): connection refused",
  })
  await expect(page.locator("button")).toHaveCount(2)
  await shot({ fit: false })
})
