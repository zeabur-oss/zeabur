import { expect, test as setup } from "./fixtures.ts"

// The launch link sets the session cookie; a plain URL works for the demo server.
setup("open the launch link", async ({ page }) => {
  const launch = process.env.ZEABUR_E2E_URL
  if (!launch) throw new Error("ZEABUR_E2E_URL is not set")
  await page.goto(launch)
  await expect(page).toHaveURL(
    (url) => url.pathname === "/" && !url.search.includes("zeabur-launch"),
  )
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeAttached()
  await page.context().storageState({ path: "e2e/.auth/state.json" })
})
