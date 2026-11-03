// The README's screenshots (sets.ts), from the demo, English only, light and dark.
import { expect, test } from "./fixtures.ts"

test("readme-home", async ({ page, shot }) => {
  await page.goto("/", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main", until: 'section[aria-labelledby="home-problems"]' })
})
test("readme-project", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("readme-diagnosis", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/worker", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("readme-database", async ({ page, shot }) => {
  await page.goto("/projects/shop/databases/postgresql/orders", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("readme-rollout", async ({ page, shot }) => {
  await page.goto("/projects/payments/services/Deployment/api", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("readme-cluster", async ({ page, shot }) => {
  await page.goto("/cluster", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main", until: "#cluster-nodes" })
})
