import type { Browser, Page } from "@playwright/test"

import { expect } from "../fixtures.ts"
import { authenticator, enrol, signInAtZeaburIdentity } from "./identity.ts"

type Grant = { project: string; role: "developer" | "viewer" }

/** Enrols from an admin's invitation, signs in, and takes on admin rights on the Cluster page. */
export async function signInAsAdmin(page: Page, link: string) {
  const identity = new URL(link).origin
  const device = await authenticator(page)
  await enrol(page, link)
  await page.goto("/")
  await signInAtZeaburIdentity(page, identity)
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
  await page.goto("/cluster")
  const nodes = page.getByRole("region", { name: "Nodes" })
  const view = nodes.getByRole("button", { name: "Use admin rights for a few minutes" })
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
}

/** Invites `username` with Zeabur's API from the page, as the interface does: plans the
 * invitation, then applies it. */
export async function invite(page: Page, username: string, projects: Grant[]): Promise<string> {
  const answer = await page.evaluate(
    async (change) => {
      const post = async (path: string, body: unknown) => {
        const response = await fetch(`/api/v1/members/${path}`, {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json", "zeabur-request": "1" },
          body: JSON.stringify(body),
        })
        return { status: response.status, body: (await response.json()) as Record<string, unknown> }
      }
      const plan = await post("plan", change)
      if (plan.status !== 200) return plan
      return post("apply", { token: plan.body.token })
    },
    { op: "invite", username, projects },
  )
  expect(answer.status, JSON.stringify(answer.body)).toBe(200)
  const link = answer.body.link as { url: string; createdMember: boolean }
  expect(link.createdMember).toBe(true)
  return link.url
}

/** A plan's token, or a problem. */
type Answer = { token?: string; kind?: string; why?: { params?: Record<string, string> } }

/** Plans `changes` in `project` and applies the plan, with Zeabur's API from the page. */
export async function planAndApply(page: Page, project: string, changes: unknown[]) {
  return page.evaluate(
    async ({ project, changes }) => {
      const post = async (path: string, body: unknown) => {
        const response = await fetch(`/api/v1/projects/${encodeURIComponent(project)}/${path}`, {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json", "zeabur-request": "1" },
          body: JSON.stringify(body),
        })
        return { status: response.status, body: (await response.json()) as Answer }
      }
      const plan = await post("plan", { changes })
      if (plan.status !== 200) return { stage: "plan", ...plan }
      return { stage: "apply", ...(await post("apply", { token: plan.body.token })) }
    },
    { project, changes },
  )
}

/** A browser of the member's own: enrols from `link`, signs in, and checks who they are. */
export async function member(browser: Browser, link: string, username: string, zeabur: string) {
  const context = await browser.newContext({ baseURL: zeabur, locale: "en-US" })
  const page = await context.newPage()
  await authenticator(page)
  await enrol(page, link)
  await page.goto("/")
  await signInAtZeaburIdentity(page, new URL(link).origin)
  await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible()
  const whoami = (await (await page.request.get("/api/v1/whoami")).json()) as {
    username: string
    groups: string[]
  }
  expect(whoami.username).toBe(`zeabur:${username}`)
  expect(whoami.groups).not.toContain("zeabur:admins")
  return page
}

/** The newest toast titled `title`. */
export const toast = (page: Page, title: string) =>
  page
    .getByRole("region", { name: /^Notifications/ })
    .getByRole("listitem")
    .filter({ hasText: title })
    .first()
