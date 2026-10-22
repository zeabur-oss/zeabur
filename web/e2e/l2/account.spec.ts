import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { authenticator, enrol, signInAtZeaburIdentity } from "./identity.ts"

// A member manages their passkeys on Zeabur Identity's account page, reached from Zeabur's.
// scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL:
//
//   ZEABUR_L2_MEMBER_LINK   an invitation `zeabur-identityctl invite` printed for `e2e-member`,
//                           in no group
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const memberLink = env("ZEABUR_L2_MEMBER_LINK")
const zeabur = env("ZEABUR_L2_URL")
const identity = memberLink ? new URL(memberLink).origin : ""

test.beforeAll(() => {
  if (!memberLink) throw new Error("ZEABUR_L2_MEMBER_LINK is not set")
})

const passkeys = (page: Page) => page.getByRole("region", { name: "Passkeys" })
const sessions = (page: Page) => page.getByRole("region", { name: "Sessions" })
const passkey = (page: Page, label: string) =>
  passkeys(page).getByRole("listitem").filter({ hasText: label })
const confirmRemove = async (row: ReturnType<typeof passkey>) => {
  await row.getByRole("button", { name: "Remove" }).click()
  await expect(row.getByText("Remove this passkey?")).toBeVisible()
  await row.locator("[data-confirm]").getByRole("button", { name: "Remove" }).click()
}

async function signInToAccount(page: Page) {
  await page.getByRole("button", { name: "Sign in with a passkey" }).click()
  await expect(page.getByText("Signed in as e2e-member")).toBeVisible({ timeout: 30_000 })
}

test("a member renames, adds and removes passkeys, and cannot remove the last one", async ({
  page,
  browser,
}) => {
  test.setTimeout(180_000)
  // The member's laptop: they enrol with it and sign in to Zeabur with it.
  const laptop = await authenticator(page)

  const [accountPage, laptopOnAccountPage] =
    await test.step("the member signs in to Zeabur and follows the link to manage sign-in", async () => {
      await enrol(page, memberLink)
      await page.goto("/")
      await signInAtZeaburIdentity(page, identity)
      await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()

      await page.goto("/account")
      await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible()
      const manage = page.getByRole("link", { name: /^Manage sign-in methods/ })
      await expect(manage).toHaveAttribute("href", `${identity}/account`)
      const opened = page.waitForEvent("popup")
      await manage.click()
      const account = await opened
      const device = await authenticator(account, await laptop.passkeys())
      await expect(account).toHaveURL(`${identity}/account`)
      await signInToAccount(account)
      return [account, device] as const
    })

  await test.step("their one passkey is listed, cannot be removed, and takes a new name", async () => {
    const only = passkeys(accountPage).getByRole("listitem")
    await expect(only).toHaveCount(1)
    await expect(passkeys(accountPage).getByText("Add a backup passkey")).toBeVisible()
    await expect(only).toContainText("This device")
    await expect(only).toContainText("This is your only passkey, so it can’t be removed.")
    await expect(only.getByRole("button", { name: "Remove" })).toHaveCount(0)

    await only.getByRole("button", { name: "Rename" }).click()
    await only.getByRole("textbox", { name: "Passkey name" }).fill("Work laptop")
    await only.getByRole("button", { name: "Save" }).click()
    await expect(passkey(accountPage, "Work laptop")).toBeVisible({ timeout: 30_000 })
    await expect(
      sessions(accountPage).getByRole("listitem").filter({ hasText: "Zeabur" }),
    ).toContainText("Work laptop")
  })

  // The page excludes passkeys the member already has, so the laptop is put away meanwhile.
  const phone = await test.step("they add a second passkey on their phone", async () => {
    await laptopOnAccountPage.remove()
    const device = await authenticator(accountPage)
    await passkeys(accountPage).getByRole("textbox", { name: "Passkey name" }).fill("Phone")
    await passkeys(accountPage).getByRole("button", { name: "Add a passkey" }).click()
    await expect(passkeys(accountPage).getByRole("listitem")).toHaveCount(2, { timeout: 30_000 })
    await expect(passkey(accountPage, "Phone")).toBeVisible()
    await expect(passkeys(accountPage).getByText("Add a backup passkey")).toBeHidden()
    expect(await device.passkeys()).toHaveLength(1)
    return device
  })

  // On the phone, the account page lists both passkeys and stays open, not reloaded.
  const other = await browser.newContext({ baseURL: identity, locale: "en-US" })
  const onPhone = await other.newPage()
  await authenticator(onPhone, await phone.passkeys())
  await onPhone.goto("/account")
  await signInToAccount(onPhone)
  await expect(passkeys(onPhone).getByRole("listitem")).toHaveCount(2)

  await test.step("removing the laptop's passkey signs out what it signed in", async () => {
    // Signing in a moment ago counts as the passkey confirmation, which lasts five minutes;
    // after that the page asks for one first (Zeabur Identity's own tests cover it).
    await confirmRemove(passkey(accountPage, "Work laptop"))
    // The account page's own session signed in with that passkey.
    await expect(
      accountPage.getByText("Sign in to see your passkeys and where you’re signed in."),
    ).toBeVisible({ timeout: 30_000 })
  })

  await test.step("the phone's page still lists two, but the last passkey is refused", async () => {
    const row = passkey(onPhone, "Phone")
    await confirmRemove(row)
    await expect(onPhone.locator("#error")).toHaveText(
      "You can’t remove your only passkey. Add another first.",
      { timeout: 30_000 },
    )

    await onPhone.reload()
    await expect(passkeys(onPhone).getByRole("listitem")).toHaveCount(1)
    await expect(row).toContainText("This is your only passkey, so it can’t be removed.")
    await expect(row.getByRole("button", { name: "Remove" })).toHaveCount(0)
    // Zeabur renews its session with a grant the removed passkey made, so that grant is revoked:
    // the access token Zeabur holds works until it expires, within minutes, then renewing fails.
    await expect(sessions(onPhone).getByRole("listitem").filter({ hasText: "Zeabur" })).toHaveCount(
      0,
    )
    await other.close()
  })

  await test.step("the removed passkey no longer signs in to Zeabur", async () => {
    const elsewhere = await browser.newContext({ baseURL: zeabur, locale: "en-US" })
    const signIn = await elsewhere.newPage()
    await authenticator(signIn, await laptop.passkeys())
    // The page's passkey autofill may send the passkey before the button is pressed.
    const answered = signIn.waitForResponse(
      (response) =>
        response.url() === `${identity}/login` && response.request().method() === "POST",
    )
    await signIn.goto("/")
    await expect(signIn).toHaveURL((url) => url.origin === identity)
    await Promise.race([
      answered,
      signIn
        .locator('#sign-in button[type="submit"]')
        .click()
        .catch(() => answered),
    ])
    expect((await answered).ok()).toBe(false)
    await expect(signIn.locator("#error")).toHaveText(
      "Unable to verify the passkey. Try again or use another passkey.",
    )
    await expect(signIn).toHaveURL((url) => url.origin === identity)
    await elsewhere.close()
  })
})
