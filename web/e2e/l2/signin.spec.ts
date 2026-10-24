import { expect, test } from "../fixtures.ts"
import { authenticator, enrol, signInAtZeaburIdentity } from "./identity.ts"

// Zeabur in a cluster with Zeabur Identity, first sign-in to admin rights. scripts/e2e-l2.sh sets:
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

test("a first admin enrols a passkey, signs in, confirms a terminal with it and sees what was refused", async ({
  page,
}) => {
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
    expect(you.groups).not.toContain("zeabur:elevated:admins")
    await expect(page.getByRole("button", { name: /, signed in as admin$/ })).toContainText("admin")
  })

  await test.step("before elevating, the cluster's nodes are refused with Use admin rights", async () => {
    // Without elevation the admins group has `view`, which does not list nodes.
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    await expect(nodes.getByText("Seeing the nodes needs admin rights.")).toBeVisible({
      timeout: 30_000,
    })
    await expect(
      nodes.getByRole("button", { name: "Use admin rights for a few minutes" }),
    ).toBeVisible()
  })

  await test.step("the terminal asks for the passkey, then runs a command in the pod", async () => {
    // Cross-Origin-Opener-Policy: same-origin severs window.opener on the trip through Zeabur
    // Identity, so /auth/callback reports on BroadcastChannel("zeabur-elevation") and the popup
    // may or may not close itself.
    await page.goto(servicePage)
    await expect(page.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await page
      .locator("main header")
      .first()
      .getByRole("button", { name: "Terminal", exact: true })
      .click()
    const dock = page.getByRole("region", { name: "Terminal" })
    await expect(dock.getByRole("tab", { name: service })).toHaveAttribute("aria-selected", "true")
    // Scoped to the dock: refused Secrets elsewhere on the page also offer admin rights.
    const panel = dock.getByRole("tabpanel", { name: service })
    await expect(panel.getByText("Opening a terminal needs admin rights.")).toBeVisible({
      timeout: 30_000,
    })
    const retry = panel.getByRole("button", { name: "Retry with admin rights" })
    await expect(retry).toBeVisible()
    const opened = page.waitForEvent("popup")
    await retry.click()
    const popup = await opened
    await expect(page.getByRole("dialog", { name: "Admin rights" })).toBeVisible()
    await authenticator(popup, await device.passkeys())
    await signInAtZeaburIdentity(popup, identity)
    await expect(page.getByRole("dialog", { name: "Admin rights" })).toBeHidden({
      timeout: 30_000,
    })
    await expect(page.getByText(/^Admin until /).first()).toBeVisible()
    if (!popup.isClosed()) {
      expect(new URL(popup.url()).pathname).toBe("/auth/callback")
      await popup.close()
    }
    const terminal = dock.getByRole("group", { name: "Terminal" })
    await expect(dock.getByRole("status").filter({ hasText: "Connected" })).toBeVisible({
      timeout: 30_000,
    })
    await terminal.click()
    // The shell computes the answer, so the typed command's echo cannot satisfy the check.
    await page.keyboard.type("echo hi-$((40 + 2))\n")
    await expect(terminal).toContainText("hi-42", { timeout: 30_000 })
  })

  await test.step("with admin rights, every request carries them: the nodes appear", async () => {
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    await expect(nodes.getByRole("list", { name: "Nodes" })).toBeVisible({ timeout: 30_000 })
    await expect(nodes.getByText("Seeing the nodes needs admin rights.")).toBeHidden()
    const whoami = (await (await page.request.get("/api/v1/whoami")).json()) as {
      groups: string[]
    }
    expect(whoami.groups).toContain("zeabur:elevated:admins")
  })

  await test.step("a restart applies with the same admin rights", async () => {
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
