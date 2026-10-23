import { expect, test } from "../fixtures.ts"
import { deleteService } from "./git.ts"
import { invite, member, signInAsAdmin, toast } from "./members.ts"

// Deploy an image, then delete it: the delete plan lists what the service made (its Kubernetes
// Service, `-env` Secret, Ingress and volume claim) and, once applied, none of them is left.
// scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL and ZEABUR_L2_NAMESPACE:
//
//   ZEABUR_L2_IMAGE_ADMIN_LINK  an invitation for `e2e-image-admin`, in the groups admins and
//                               elevated admins
//   ZEABUR_L2_IMAGE             the image, by default traefik/whoami:v1.10.3 (port 80)
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const adminLink = env("ZEABUR_L2_IMAGE_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const zeabur = env("ZEABUR_L2_URL")
const image = env("ZEABUR_L2_IMAGE", "traefik/whoami:v1.10.3")

test.skip(!adminLink, "no invitation for e2e-image-admin")
test.use({ actionTimeout: 30_000 })

const name = `img-${Date.now().toString(36)}`

test("a Developer deploys an image and deletes it with what it made", async ({ page, browser }) => {
  test.setTimeout(600_000)
  let developerLink = ""
  await test.step("the admin invites a Developer", async () => {
    await signInAsAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-image-developer", [{ project, role: "developer" }])
  })
  const developer = await member(browser, developerLink, "e2e-image-developer", zeabur)
  let made = false
  try {
    await test.step("the Developer deploys an image with a port, a domain, a variable and a volume", async () => {
      await developer.goto(`/projects/${encodeURIComponent(project)}`)
      await developer.getByRole("button", { name: "New", exact: true }).click()
      await developer.getByRole("menuitem", { name: "Docker image", exact: true }).click()
      const drawer = developer.getByRole("dialog", { name: "Deploy an image" })
      await drawer.getByRole("textbox", { name: "Image" }).fill(image)
      await drawer.getByRole("textbox", { name: "Name" }).fill(name)
      await drawer.getByRole("textbox", { name: "Port" }).fill("80")
      await drawer.getByRole("switch", { name: "Public domain" }).click()
      const host = drawer.getByRole("textbox", { name: "Public domain" })
      if ((await host.inputValue()) === "") await host.fill(`${name}.example.test`)
      await drawer.getByRole("button", { name: "More settings" }).click()
      await drawer.getByRole("textbox", { name: "Variables" }).fill("GREETING=hello")
      await drawer.getByRole("switch", { name: "Volume" }).click()
      await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
      const plan = developer.getByRole("dialog", { name: `Deploy ${name}` })
      await expect(plan.getByRole("button", { name: "Back" })).toBeVisible()
      await plan.getByRole("button").last().click()
      await expect(plan).toHaveCount(0)
      await expect(toast(developer, "Applied")).toBeVisible({ timeout: 30_000 })
      made = true
    })
  } finally {
    if (made)
      await test.step(`the Developer deletes ${name}, its Ingress and its volume`, () =>
        deleteService(developer, project, name, {
          built: false,
          also: [
            `Service ${name}`,
            `Secret ${name}-env`,
            `Ingress ${name}`,
            `PersistentVolumeClaim ${name}-data`,
          ],
        }))
  }
})
