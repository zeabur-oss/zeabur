import type { Browser, Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { authenticator, enrol, signInAtZeaburIdentity } from "./identity.ts"

// Roles per project: an admin invites a Developer of one project and a Viewer of another, and
// each sees what their role allows. scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL:
//
//   ZEABUR_L2_ADMIN_LINK         an invitation `zeabur-identityctl invite` printed for `e2e-admin`,
//                                in the groups admins and elevated admins
//   ZEABUR_L2_NAMESPACE          the Developer's project, with a running Deployment
//   ZEABUR_L2_SECOND_NAMESPACE   the Viewer's project, with a running Deployment of the same name
//   ZEABUR_L2_SERVICE            that Deployment (default talker)
//
// It deletes the users e2e-developer and e2e-viewer before each run, so Zeabur makes them anew.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const adminLink = env("ZEABUR_L2_ADMIN_LINK")
const first = env("ZEABUR_L2_NAMESPACE")
const second = env("ZEABUR_L2_SECOND_NAMESPACE")
const service = env("ZEABUR_L2_SERVICE", "talker")
const zeabur = env("ZEABUR_L2_URL")
const identity = adminLink ? new URL(adminLink).origin : ""
const servicePage = (project: string) =>
  `/projects/${encodeURIComponent(project)}/services/Deployment/${service}`

test.beforeAll(() => {
  if (!adminLink) throw new Error("ZEABUR_L2_ADMIN_LINK is not set")
  if (!first) throw new Error("ZEABUR_L2_NAMESPACE is not set")
  if (!second) throw new Error("ZEABUR_L2_SECOND_NAMESPACE is not set")
})

type Grant = { project: string; role: "developer" | "viewer" }

/** Invites `username` with Zeabur's API from the page, as the interface does. */
async function invite(page: Page, username: string, projects: Grant[]): Promise<string> {
  const answer = await page.evaluate(
    async (body) => {
      const response = await fetch("/api/v1/members/invitations", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json", "zeabur-request": "1" },
        body: JSON.stringify(body),
      })
      return { status: response.status, body: (await response.json()) as unknown }
    },
    { username, projects },
  )
  expect(answer.status, JSON.stringify(answer.body)).toBe(201)
  const link = answer.body as { url: string; createdMember: boolean }
  expect(link.createdMember).toBe(true)
  return link.url
}

/** A browser of the member's own: enrols from `link`, signs in, and checks who they are. */
async function member(browser: Browser, link: string, username: string) {
  const context = await browser.newContext({ baseURL: zeabur, locale: "en-US" })
  const page = await context.newPage()
  await authenticator(page)
  await enrol(page, link)
  await page.goto("/")
  await signInAtZeaburIdentity(page, identity)
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
  const whoami = (await (await page.request.get("/api/v1/whoami")).json()) as {
    username: string
    groups: string[]
  }
  expect(whoami.username).toBe(`zeabur:${username}`)
  expect(whoami.groups).not.toContain("zeabur:admins")
  return page
}

test("an admin invites a Developer and a Viewer, who each get what their role allows", async ({
  page,
  browser,
}) => {
  test.setTimeout(300_000)
  const device = await authenticator(page)
  let developerLink = ""
  let viewerLink = ""

  await test.step("the admin signs in and takes on admin rights", async () => {
    await enrol(page, adminLink)
    await page.goto("/")
    await signInAtZeaburIdentity(page, identity)
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
    await page.goto("/cluster")
    const nodes = page.getByRole("region", { name: "Nodes" })
    const view = nodes.getByRole("button", { name: "View as admin" })
    await expect(view).toBeVisible({ timeout: 30_000 })
    const opened = page.waitForEvent("popup")
    await view.click()
    const popup = await opened
    await authenticator(popup, await device.passkeys())
    await signInAtZeaburIdentity(popup, identity)
    await expect(page.getByRole("dialog", { name: "Admin rights" })).toBeHidden({
      timeout: 30_000,
    })
    if (!popup.isClosed()) await popup.close()
    await expect(nodes.getByRole("list", { name: "Nodes" })).toBeVisible({ timeout: 30_000 })
  })

  await test.step("the admin invites a Developer of one project and a Viewer of the other", async () => {
    developerLink = await invite(page, "e2e-developer", [{ project: first, role: "developer" }])
    viewerLink = await invite(page, "e2e-viewer", [{ project: second, role: "viewer" }])
  })

  await test.step("the Developer sees both projects' names and restarts a service in theirs", async () => {
    const developer = await member(browser, developerLink, "e2e-developer")
    await developer.goto("/projects")
    const main = developer.getByRole("main")
    await expect(main.getByText(first, { exact: true }).first()).toBeVisible({ timeout: 30_000 })
    await expect(main.getByText(second, { exact: true }).first()).toBeVisible()

    await developer.goto(servicePage(first))
    await expect(developer.getByRole("heading", { level: 1, name: service })).toBeVisible()
    await developer.getByRole("button", { name: "Restart" }).click()
    const dialog = developer.getByRole("dialog", { name: `Restart ${service}` })
    await dialog.getByRole("button", { name: "Restart", exact: true }).click()
    await expect(
      developer
        .getByRole("region", { name: /^Notifications/ })
        .getByRole("listitem")
        .filter({ hasText: "Applied" })
        .first(),
    ).toBeVisible({ timeout: 30_000 })

    await developer.goto(`/projects/${encodeURIComponent(second)}`)
    await expect(developer.getByText(`You don't have access to ${second}`)).toBeVisible({
      timeout: 30_000,
    })
    await expect(
      developer.getByText("An admin can give you a role in it on the Members page."),
    ).toBeVisible()
    await developer.context().close()
  })

  await test.step("the Viewer sees their project's service but cannot open a terminal", async () => {
    const viewer = await member(browser, viewerLink, "e2e-viewer")
    await viewer.goto(servicePage(second))
    await expect(viewer.getByRole("heading", { level: 1, name: service })).toBeVisible({
      timeout: 30_000,
    })
    await viewer
      .locator("main header")
      .first()
      .getByRole("button", { name: "Terminal", exact: true })
      .click()
    const dock = viewer.getByRole("region", { name: "Terminal" })
    const panel = dock.getByRole("tabpanel", { name: service })
    // Admin rights are offered to everyone signed in here, so the sentence may name either.
    await expect(panel.getByText(/^Opening a terminal needs (admin|more) rights\./)).toBeVisible({
      timeout: 30_000,
    })
    await expect(dock.getByRole("status").filter({ hasText: "Connected" })).toBeHidden()
    await viewer.context().close()
  })
})
