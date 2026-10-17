import { expect, test as setup } from "./fixtures.ts"

// Opens the one-time launch link once; the server sets the session cookie and redirects to `/`,
// the home page.
// A plain URL (the demo server, which needs no session) works too.
setup("open the launch link", async ({ page }) => {
  const launch = process.env.ENGAWA_E2E_URL
  if (!launch) throw new Error("ENGAWA_E2E_URL is not set")
  await page.goto(launch)
  await expect(page).toHaveURL(
    (url) => url.pathname === "/" && !url.search.includes("engawa-launch"),
  )
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeAttached()
  await page.context().storageState({ path: "e2e/.auth/state.json" })
})
