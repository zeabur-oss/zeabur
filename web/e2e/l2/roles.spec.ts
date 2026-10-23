import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { invite, member, planAndApply, signInAsAdmin } from "./members.ts"

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
const servicePage = (project: string) =>
  `/projects/${encodeURIComponent(project)}/services/Deployment/${service}`

// A project card shows the project's display name, or its namespace when it has none.
const shownName = async (page: Page, project: string) => {
  const namespace = (await (
    await page.request.get(`/api/v1/projects/${encodeURIComponent(project)}/namespace`)
  ).json()) as { displayName?: string }
  return namespace.displayName ?? project
}

test.beforeAll(() => {
  if (!adminLink) throw new Error("ZEABUR_L2_ADMIN_LINK is not set")
  if (!first) throw new Error("ZEABUR_L2_NAMESPACE is not set")
  if (!second) throw new Error("ZEABUR_L2_SECOND_NAMESPACE is not set")
})

test("an admin invites a Developer and a Viewer, who each get what their role allows", async ({
  page,
  browser,
}) => {
  test.setTimeout(300_000)
  let developerLink = ""
  let viewerLink = ""
  let developer: Page

  await test.step("the admin signs in and takes on admin rights", async () => {
    await signInAsAdmin(page, adminLink)
  })

  await test.step("the admin invites a Developer of one project and a Viewer of the other", async () => {
    developerLink = await invite(page, "e2e-developer", [{ project: first, role: "developer" }])
    viewerLink = await invite(page, "e2e-viewer", [{ project: second, role: "viewer" }])
  })

  await test.step("the Developer sees both projects' names and restarts a service in theirs", async () => {
    developer = await member(browser, developerLink, "e2e-developer", zeabur)
    await developer.goto("/projects")
    const main = developer.getByRole("main")
    await expect(
      main.getByText(await shownName(developer, first), { exact: true }).first(),
    ).toBeVisible({
      timeout: 30_000,
    })
    await expect(
      main.getByText(await shownName(developer, second), { exact: true }).first(),
    ).toBeVisible()

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
  })

  await test.step("the Developer names their project, but cannot change its labels", async () => {
    const name = `Shop ${Date.now()}`
    await developer.goto(`/projects/${encodeURIComponent(first)}/settings`)
    await developer.getByRole("textbox", { name: "Project name" }).fill(name)
    await developer.getByRole("button", { name: "Save details" }).click()
    const dialog = developer.getByRole("dialog", { name: `${first} details` })
    await dialog.getByRole("button", { name: "Save details", exact: true }).click()
    await expect(
      developer
        .getByRole("region", { name: /^Notifications/ })
        .getByRole("listitem")
        .filter({ hasText: "Applied" })
        .first(),
    ).toBeVisible({ timeout: 30_000 })
    const namespace = (await (
      await developer.request.get(`/api/v1/projects/${encodeURIComponent(first)}/namespace`)
    ).json()) as { displayName?: string; canChange: string; canAdopt: string }
    expect(namespace).toMatchObject({ displayName: name, canChange: "allowed", canAdopt: "denied" })

    await developer.getByRole("button", { name: "Remove", exact: true }).click()
    const help = developer.getByRole("dialog", { name: `You can't change ${first}` })
    await expect(help).toContainText("zeabur-manage")
    await help.getByRole("button", { name: "Close" }).click()

    // Past the interface, the cluster's admission policy refuses the label change itself.
    const refused = await planAndApply(developer, first, [{ op: "unadopt" }])
    expect(refused.stage, JSON.stringify(refused.body)).toBe("apply")
    expect(refused.status).toBe(403)
    expect(refused.body.kind).toBe("policy-denied")
    expect(refused.body.why?.params?.policy).toBe("zeabur-project-namespace")

    // Zeabur Proxy reads log history as the person: past its review in their project (without
    // a log store, nothing answers it there).
    const history = await developer.request.get(
      `/api/v1/projects/${encodeURIComponent(first)}/services/deployment/${service}/logs/history`,
    )
    expect(["available", "unavailable"]).toContain(
      ((await history.json()) as { state: string }).state,
    )
    await developer.context().close()
  })

  await test.step("the Viewer sees their project's service, with what only a Developer may do closed", async () => {
    const viewer = await member(browser, viewerLink, "e2e-viewer", zeabur)
    await viewer.goto(servicePage(second))
    await expect(viewer.getByRole("heading", { level: 1, name: service })).toBeVisible({
      timeout: 30_000,
    })
    await expect(viewer.getByRole("tab", { name: "Overview" })).toBeVisible()
    await expect(viewer.getByRole("tab", { name: "Logs" })).toHaveCount(0)

    const namespace = (await (
      await viewer.request.get(`/api/v1/projects/${encodeURIComponent(second)}/namespace`)
    ).json()) as { role?: string; can: Record<string, string> }
    expect(namespace.role).toBe("viewer")
    expect(namespace.can).toMatchObject({
      deploy: "denied",
      changeServices: "denied",
      openTerminal: "denied",
      readSecrets: "denied",
      changeMembers: "denied",
    })

    // Closed rather than missing, each with the role that opens it.
    const header = viewer.locator("main header").first()
    for (const name of ["Restart", "Terminal"]) {
      const button = viewer.getByRole("button", { name, exact: true })
      await expect(header.locator(button)).toBeDisabled({ timeout: 30_000 })
      const closed = header.locator("span[tabindex='0']").filter({ has: button })
      await expect(closed).toContainText("Needs Developer access")
      // Base UI's tooltip popup carries no tooltip role: it is one more line saying the same.
      const said = viewer.getByText("Needs Developer access", { exact: true })
      const before = await said.count()
      await closed.hover()
      await expect(said).toHaveCount(before + 1)
      await viewer.mouse.move(0, 0)
      await expect(said).toHaveCount(before)
    }
    await expect(viewer.getByRole("region", { name: "Terminal" })).toHaveCount(0)

    await header.getByRole("button", { name: "More" }).click()
    const remove = viewer.getByRole("menuitem", { name: /^Delete/ })
    await expect(remove).toHaveAttribute("aria-disabled", "true")
    await expect(remove).toContainText("Needs Developer access")
    await viewer.keyboard.press("Escape")

    await viewer.goto(`/projects/${encodeURIComponent(second)}`)
    const add = viewer.getByRole("main").getByRole("button", { name: "New", exact: true })
    await expect(add).toBeDisabled({ timeout: 30_000 })

    // Zeabur Proxy reads log history as the person; a Viewer may read it in no project.
    const logHistory = async (project: string) =>
      (
        (await (
          await viewer.request.get(
            `/api/v1/projects/${encodeURIComponent(project)}/services/deployment/${service}/logs/history`,
          )
        ).json()) as { state: string }
      ).state
    expect(await logHistory(second)).toBe("unreadable")
    expect(await logHistory(first)).toBe("unreadable")
    await viewer.context().close()
  })
})
