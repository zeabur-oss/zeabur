import type { CDPSession, Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"

// Zeabur in a cluster with Zeabur Identity, first sign-in to admin rights. scripts/e2e-l2.sh sets:
//
//   ZEABUR_L2_URL          Zeabur's public URL (https://zeabur.localhost)
//   ZEABUR_L2_SETUP_LINK   the one-time link `zeabur-identityctl setup-link` printed for user `admin`
//   ZEABUR_L2_NAMESPACE    a project with a running Deployment whose image has a shell
//   ZEABUR_L2_SERVICE      that Deployment (default talker)
//
// The passkey lives in Chromium's virtual authenticator, a user-verifying platform authenticator
// (https://chromedevtools.github.io/devtools-protocol/tot/WebAuthn/).
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

async function stored(cdp: CDPSession, authenticatorId: string) {
  return (await cdp.send("WebAuthn.getCredentials", { authenticatorId })).credentials
}
type Passkey = Awaited<ReturnType<typeof stored>>[number]

/** Each page (a popup too) has authenticators of its own, so a popup gets a copy of the passkeys. */
async function authenticator(page: Page, passkeys: Passkey[] = []) {
  const cdp = await page.context().newCDPSession(page)
  await cdp.send("WebAuthn.enable", { enableUI: false })
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  })
  for (const credential of passkeys) {
    await cdp.send("WebAuthn.addCredential", { authenticatorId, credential })
  }
  return { passkeys: () => stored(cdp, authenticatorId) }
}

/** The virtual authenticator may answer the page's autofill (conditional mediation) before the
 * button is clicked; either way counts. */
async function signInAtZeaburIdentity(page: Page) {
  await expect(page).toHaveURL((url) => url.origin === identity)
  const left = page
    .waitForURL((url) => url.origin !== identity, { timeout: 30_000 })
    // The elevation window may close itself as soon as it is back.
    .catch((error: unknown) => {
      if (!page.isClosed()) throw error
    })
  await Promise.race([
    left,
    page
      .locator('#sign-in button[type="submit"]')
      .click({ timeout: 30_000 })
      .catch(() => left),
  ])
  await left
}

test("a first admin enrols a passkey, signs in, confirms a terminal with it and sees what was refused", async ({
  page,
}) => {
  test.setTimeout(240_000)
  const device = await authenticator(page)

  await test.step("enrol a passkey from the setup link", async () => {
    await page.goto(setupLink)
    await page.locator('#setup button[type="submit"]').click()
    await expect(page.locator("#done")).toBeVisible()
    expect(await device.passkeys()).toHaveLength(1)
  })

  await test.step("signing in to Zeabur goes through Zeabur Identity and shows who you are", async () => {
    await page.goto("/")
    await signInAtZeaburIdentity(page)
    await expect(page).toHaveURL(
      (url) => url.origin === new URL(zeabur).origin && url.pathname === "/",
    )
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
    const whoami = await page.request.get("/api/v1/whoami")
    expect(whoami.status()).toBe(200)
    const identity = (await whoami.json()) as { username: string; groups: string[] }
    expect(identity.username).toBe("zeabur:admin")
    expect(identity.groups).toContain("zeabur:admins")
    expect(identity.groups).not.toContain("zeabur:elevated:admins")
    await expect(page.getByRole("button", { name: "Account" })).toContainText(identity.username)
  })

  await test.step("before elevating, the cluster's nodes are refused with View as admin", async () => {
    // Without elevation the admins group has `view`, which does not list nodes.
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    await expect(nodes.getByText("Seeing the nodes needs admin rights.")).toBeVisible({
      timeout: 30_000,
    })
    await expect(nodes.getByRole("button", { name: "View as admin" })).toBeVisible()
  })

  await test.step("the terminal asks for the passkey, then runs a command in the pod", async () => {
    // Cross-Origin-Opener-Policy: same-origin severs window.opener on the trip through Zeabur
    // Identity, so /auth/callback reports on BroadcastChannel("zeabur-elevation") and the popup
    // may or may not close itself.
    await page.goto(servicePage)
    await expect(page.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await page.getByRole("button", { name: "More actions" }).click()
    await page.getByRole("menuitem", { name: "Open terminal" }).click()
    const dock = page.getByRole("region", { name: "Terminal" })
    await expect(dock.getByRole("tab", { name: service })).toHaveAttribute("aria-selected", "true")
    // Scoped to the dock: refused Secrets elsewhere on the page also offer admin rights.
    const panel = dock.getByRole("tabpanel", { name: service })
    await expect(panel.getByText("Opening a terminal needs admin rights.")).toBeVisible({
      timeout: 30_000,
    })
    const retry = panel.getByRole("button", { name: "Retry as admin" })
    await expect(retry).toBeVisible()
    const opened = page.waitForEvent("popup")
    await retry.click()
    const popup = await opened
    await expect(page.getByRole("dialog", { name: "Admin rights" })).toBeVisible()
    await authenticator(popup, await device.passkeys())
    await signInAtZeaburIdentity(popup)
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
