import type { CDPSession, Page } from "@playwright/test"

import { expect } from "../fixtures.ts"

// Passkeys live in Chromium's virtual authenticator, a user-verifying platform authenticator
// (https://chromedevtools.github.io/devtools-protocol/tot/WebAuthn/).

async function stored(cdp: CDPSession, authenticatorId: string) {
  return (await cdp.send("WebAuthn.getCredentials", { authenticatorId })).credentials
}
type Passkey = Awaited<ReturnType<typeof stored>>[number]

/** Each page (a popup too) has authenticators of its own, so a popup gets a copy of the passkeys. */
export async function authenticator(page: Page, passkeys: Passkey[] = []) {
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

/** Opens a setup link (`zeabur-identityctl setup-link`, an invitation) and adds a passkey. */
export async function enrol(page: Page, link: string) {
  await page.goto(link)
  await page.locator('#setup button[type="submit"]').click()
  await expect(page.locator("#done")).toBeVisible()
}

/** The virtual authenticator may answer the page's autofill (conditional mediation) before the
 * button is clicked; either way counts. */
export async function signInAtZeaburIdentity(page: Page, identity: string) {
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
