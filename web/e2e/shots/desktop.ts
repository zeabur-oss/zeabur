// The desktop app's screenshots (sets.ts): the pages of desktop/web in a browser, with the Tauri
// commands answered by desktop/web/scripts/shots-demo.ts. A window's size is the app's, so the
// pages are not fitted.
import { execFileSync } from "node:child_process"
import { join } from "node:path"
import type { Page } from "@playwright/test"
import { expect, test } from "./fixtures.ts"

type Scenario = { sources: boolean; failure?: string }

// desktop/web/scripts/shots-demo.ts as one script for the page. It is bundled from desktop/web,
// which has the Tauri packages; `web` has none.
const web = join(import.meta.dirname, "../../../desktop/web")
const script = execFileSync(
  "bun",
  ["build", "--target=browser", "--format=iife", "scripts/shots-demo.ts"],
  { cwd: web, encoding: "utf8" },
)

// A Mac's user agent: the first-run page names the kubeconfig path by the system.
const mac =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
test.use({ app: "desktop", viewport: { width: 1100, height: 720 }, userAgent: mac })

async function open(page: Page, path: string, scenario: Scenario) {
  await page.addInitScript(`${script};demo.install(${JSON.stringify(scenario)})`)
  await page.goto(path, { waitUntil: "networkidle" })
}

// Every row is listed.
async function listed(page: Page) {
  await expect(page.locator("main li")).toHaveCount(6)
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

test("desktop-cluster", async ({ page, shot }) => {
  await open(page, "/cluster.html?name=zeabur-demo", {
    sources: true,
    failure: "error sending request for url (http://127.0.0.1:8080/version): connection refused",
  })
  await expect(page.locator("button")).toHaveCount(2)
  await shot({ fit: false })
})
