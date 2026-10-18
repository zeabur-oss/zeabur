import type { CDPSession, Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"

// engawa in the cluster, signing in through Genkan, as one person would use it the first time:
// enrol a passkey from Genkan's setup link, sign in, find that the terminal and the cluster's
// nodes need the passkey again, confirm it from the terminal and run a command, see the nodes,
// and restart a service with the same admin rights. scripts/e2e-l2.sh installs both and sets:
//
//   ENGAWA_L2_URL          engawa's public URL (https://engawa.localhost)
//   ENGAWA_L2_SETUP_LINK   the one-time link `genkanctl setup-link` printed for user `admin`
//   ENGAWA_L2_NAMESPACE    a project with a running Deployment whose image has a shell
//   ENGAWA_L2_SERVICE      that Deployment (default talker)
//
// The passkey lives in Chromium's virtual authenticator (DevTools protocol, WebAuthn domain:
// https://chromedevtools.github.io/devtools-protocol/tot/WebAuthn/): a platform authenticator
// (internal transport) that verifies the user, as Touch ID or Windows Hello would. The steps
// after sign-in use the elevation and terminal interfaces by their roles and names.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const setupLink = env("ENGAWA_L2_SETUP_LINK")
const namespace = env("ENGAWA_L2_NAMESPACE")
const service = env("ENGAWA_L2_SERVICE", "talker")
const servicePage = `/projects/${encodeURIComponent(namespace)}/services/Deployment/${service}`
const engawa = env("ENGAWA_L2_URL")
const genkan = setupLink ? new URL(setupLink).origin : ""

test.beforeAll(() => {
  if (!setupLink) throw new Error("ENGAWA_L2_SETUP_LINK is not set")
  if (!namespace) throw new Error("ENGAWA_L2_NAMESPACE is not set")
})

async function stored(cdp: CDPSession, authenticatorId: string) {
  return (await cdp.send("WebAuthn.getCredentials", { authenticatorId })).credentials
}
type Passkey = Awaited<ReturnType<typeof stored>>[number]

/** Gives `page` a virtual platform authenticator holding `passkeys`. Each page (a popup too)
 * has authenticators of its own, so a popup gets a copy of the passkeys. */
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

/** Genkan's sign-in page, until it leaves for engawa. The page also offers the passkey through
 * autofill (conditional mediation), which the virtual authenticator, presence simulated, may
 * answer by itself before the button is clicked; either way counts. */
async function signInAtGenkan(page: Page) {
  await expect(page).toHaveURL((url) => url.origin === genkan)
  const left = page
    .waitForURL((url) => url.origin !== genkan, { timeout: 30_000 })
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

  await test.step("signing in to engawa goes through Genkan and shows who you are", async () => {
    await page.goto("/")
    await signInAtGenkan(page)
    await expect(page).toHaveURL(
      (url) => url.origin === new URL(engawa).origin && url.pathname === "/",
    )
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
    const whoami = await page.request.get("/api/v1/whoami")
    expect(whoami.status()).toBe(200)
    const identity = (await whoami.json()) as { username: string; groups: string[] }
    expect(identity.username).toBe("genkan:admin")
    expect(identity.groups).toContain("genkan:admins")
    expect(identity.groups).not.toContain("genkan:elevated:admins")
    // The sidebar's account menu (only where people sign in) shows the user name.
    await expect(page.getByRole("button", { name: "Account" })).toContainText(identity.username)
  })

  await test.step("before elevating, the cluster's nodes are refused with View as admin", async () => {
    // Genkan's admins group has the `view` role day to day, which does not list nodes.
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    await expect(nodes.getByText("Seeing the nodes needs admin rights.")).toBeVisible({
      timeout: 30_000,
    })
    await expect(nodes.getByRole("button", { name: "View as admin" })).toBeVisible()
  })

  await test.step("the terminal asks for the passkey, then runs a command in the pod", async () => {
    // Exec takes `get` on pods/exec, which `view` lacks: the terminal's error frame offers
    // "Retry as admin", which opens /auth/elevate in a window of its own, which goes to Genkan for
    // the passkey. engawa's page sets Cross-Origin-Opener-Policy: same-origin, so that trip
    // severs window.opener: the window's last page (/auth/callback) posts the outcome on
    // BroadcastChannel("engawa-elevation") and tries to close itself, and this page reconnects
    // on that message, whether or not the window could close.
    await page.goto(servicePage)
    await expect(page.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await page
      .getByRole("tablist", { name: "Service sections" })
      .getByRole("tab", { name: "Terminal" })
      .click()
    // The terminal's own refusal, not another one on the page (Secrets the `view` role cannot
    // list also offer admin rights): wait for it inside the Terminal tab, then use its button.
    // The terminal reconnects on its own once the rights are active, whichever button granted them.
    const panel = page.getByRole("tabpanel", { name: "Terminal" })
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
    await signInAtGenkan(popup)
    await expect(page.getByRole("dialog", { name: "Admin rights" })).toBeHidden({
      timeout: 30_000,
    })
    await expect(page.getByText(/^Admin until /).first()).toBeVisible()
    if (!popup.isClosed()) {
      expect(new URL(popup.url()).pathname).toBe("/auth/callback")
      await popup.close()
    }
    const terminal = page.getByRole("group", { name: "Terminal" })
    await expect(page.getByRole("status").filter({ hasText: "Connected" })).toBeVisible({
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
    expect(whoami.groups).toContain("genkan:elevated:admins")
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
