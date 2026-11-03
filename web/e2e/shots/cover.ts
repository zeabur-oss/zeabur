// The plate of Zeabur's README cover (sets.ts): the review of a new service, at the plate's size.
// Render the cover in design-system afterwards (covers/README.md).
import { expect, test } from "./fixtures.ts"

test.use({ viewport: { width: 544, height: 544 } })

test("shot", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-image"]').click()
  await page
    .locator('[data-slot="dialog-content"] input')
    .first()
    .fill("docker.io/nginxinc/nginx-unprivileged:1.30.5-alpine-slim")
  await page.locator('[data-slot="dialog-content"] input').nth(1).fill("blog")
  await page.locator('[data-slot="dialog-content"] input').nth(2).fill("8080")
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await expect(
    page.locator('[data-slot="dialog-content"] input[value$="example.com"]'),
  ).toBeVisible()
  await page.locator('[data-slot="dialog-footer"] button:last-child').click()
  await page.locator('[data-slot="dialog-body"] > :not([hidden]) button:text-is("YAML")').click()
  await shot({ fit: false })
})
