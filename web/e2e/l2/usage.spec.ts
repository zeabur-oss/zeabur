import { expect, test } from "../fixtures.ts"
import { invite, member, signInAsAdmin } from "./members.ts"

// The cluster page's resource usage: an admin sees the test project and its services; a Viewer
// of one project may not list every pod, so the section says it needs more rights.
// scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL:
//
//   ZEABUR_L2_USAGE_ADMIN_LINK   an invitation for `e2e-usage-admin`, in the groups admins and
//                                elevated admins
//   ZEABUR_L2_NAMESPACE          a project with a running Deployment
//   ZEABUR_L2_SERVICE            that Deployment (default talker)
//
// It deletes the user e2e-usage-viewer before each run, so Zeabur makes it anew.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const adminLink = env("ZEABUR_L2_USAGE_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const service = env("ZEABUR_L2_SERVICE", "talker")
const zeabur = env("ZEABUR_L2_URL")

test.beforeAll(() => {
  if (!adminLink) throw new Error("ZEABUR_L2_USAGE_ADMIN_LINK is not set")
  if (!project) throw new Error("ZEABUR_L2_NAMESPACE is not set")
})

test("an admin sees what the test project uses; a Viewer is told it needs more rights", async ({
  page,
  browser,
}) => {
  test.setTimeout(240_000)

  await test.step("the admin signs in and takes on admin rights", async () => {
    await signInAsAdmin(page, adminLink)
  })

  await test.step("the admin sees the project and opens its services", async () => {
    const usage = page.getByRole("region", { name: "Resource usage" })
    const namespace = (await (
      await page.request.get(`/api/v1/projects/${encodeURIComponent(project)}/namespace`)
    ).json()) as { displayName?: string }
    const row = usage
      .getByRole("listitem")
      .filter({ has: page.getByRole("link", { name: namespace.displayName ?? project }) })
    await expect(row).toBeVisible({ timeout: 30_000 })
    await row.getByRole("button", { name: "Services" }).click()
    await expect(
      row.getByRole("list", { name: "Services" }).getByRole("link", { name: service }),
    ).toBeVisible({ timeout: 30_000 })
  })

  await test.step("a Viewer of the project sees that usage needs more rights", async () => {
    const link = await invite(page, "e2e-usage-viewer", [{ project, role: "viewer" }])
    const viewer = await member(browser, link, "e2e-usage-viewer", zeabur)
    await viewer.goto("/cluster")
    await expect(
      viewer
        .getByRole("region", { name: "Resource usage" })
        .getByText("Seeing what each namespace uses needs more rights."),
    ).toBeVisible({ timeout: 30_000 })
  })
})
