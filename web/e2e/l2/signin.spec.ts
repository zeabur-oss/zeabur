import { expect, test } from "../fixtures.ts"
import { authenticator, enrol, signInAtZeaburIdentity } from "./identity.ts"

// Zeabur in a cluster with Zeabur Identity, the first admin account signs in. scripts/e2e-l2-browser.sh sets:
//
//   ZEABUR_L2_URL          Zeabur's public URL (https://zeabur.localhost)
//   ZEABUR_L2_SETUP_LINK   the one-time link `zeabur-identityctl setup-link` printed for user `admin`
//   ZEABUR_L2_NAMESPACE    a project with a running Deployment whose image has a shell
//   ZEABUR_L2_SERVICE      that Deployment (default talker)
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const setupLink = env("ZEABUR_L2_SETUP_LINK")
const namespace = env("ZEABUR_L2_NAMESPACE")
const service = env("ZEABUR_L2_SERVICE", "talker")
const servicePage = `/projects/${encodeURIComponent(namespace)}/services/Deployment/${service}`
const zeabur = env("ZEABUR_L2_URL")
const identity = setupLink ? new URL(setupLink).origin : ""

test.beforeAll(() => {
  if (!setupLink) throw new Error("ZEABUR_L2_SETUP_LINK is not set")
  if (!namespace) throw new Error("ZEABUR_L2_NAMESPACE is not set")
})

test("a first admin enrols a passkey, signs in and acts with full rights", async ({ page }) => {
  test.setTimeout(240_000)
  const device = await authenticator(page)

  await test.step("enrol a passkey from the setup link", async () => {
    await enrol(page, setupLink)
    expect(await device.passkeys()).not.toEqual([])
  })

  await test.step("signing in to Zeabur goes through Zeabur Identity and shows who you are", async () => {
    await page.goto("/")
    await signInAtZeaburIdentity(page, identity)
    await expect(page).toHaveURL(
      (url) => url.origin === new URL(zeabur).origin && url.pathname === "/",
    )
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
    const whoami = await page.request.get("/api/v1/whoami")
    expect(whoami.status()).toBe(200)
    const you = (await whoami.json()) as { username: string; groups: string[] }
    expect(you.username).toBe("zeabur:admin")
    expect(you.groups).toContain("zeabur:admins")
    await expect(page.getByRole("button", { name: /, signed in as admin$/ })).toContainText("admin")
  })

  await test.step("an admin account sees the cluster's nodes without asking for more", async () => {
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    await expect(nodes.getByRole("list", { name: "Nodes" })).toBeVisible({ timeout: 30_000 })
    await expect(nodes.getByText("Seeing the nodes needs more rights.")).toBeHidden()
  })

  await test.step("the terminal runs a command in the pod", async () => {
    await page.goto(servicePage)
    await expect(page.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await page
      .locator("main header")
      .first()
      .getByRole("button", { name: "Terminal", exact: true })
      .click()
    const dock = page.getByRole("region", { name: "Terminal" })
    await expect(dock.getByRole("tab", { name: service })).toHaveAttribute("aria-selected", "true")
    const terminal = dock.getByRole("group", { name: "Terminal" })
    await expect(dock.getByRole("status").filter({ hasText: "Connected" })).toBeVisible({
      timeout: 30_000,
    })
    await terminal.click()
    // The shell computes the answer, so the typed command's echo cannot satisfy the check.
    await page.keyboard.type("echo hi-$((40 + 2))\n")
    await expect(terminal).toContainText("hi-42", { timeout: 30_000 })
  })

  await test.step("a restart applies", async () => {
    await page.goto(servicePage)
    await expect(page.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await page.getByRole("button", { name: "Restart" }).click()
    const dialog = page.getByRole("dialog", { name: `Restart ${service}` })
    await dialog.getByRole("button", { name: "Restart", exact: true }).click()
    await expect(
      page
        .getByRole("region", { name: /^Notifications/ })
        .getByRole("listitem")
        .filter({ hasText: "Applied" })
        .first(),
    ).toBeVisible({ timeout: 30_000 })
  })
})
