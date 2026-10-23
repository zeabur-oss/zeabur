import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { deleteService } from "./git.ts"
import { invite, member, signInAsAdmin, toast } from "./members.ts"

// Deploy from Git: a Developer deploys a small public repository with its Dockerfile from its
// default branch; the service is building until Zeabur Builder pushes the first image, then Keel
// moves it there and it runs. A branch that does not exist fails the service at once.
// scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL and ZEABUR_L2_NAMESPACE:
//
//   ZEABUR_L2_GIT_ADMIN_LINK  an invitation for `e2e-git-admin`, in the groups admins and elevated
//                             admins; set only when Zeabur Builder, a registry and Keel run here
//   ZEABUR_L2_GIT_REPOSITORY  the repository, by default traefik/whoami (Dockerfile, port 80)
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const adminLink = env("ZEABUR_L2_GIT_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const zeabur = env("ZEABUR_L2_URL")
const repository = env("ZEABUR_L2_GIT_REPOSITORY", "https://github.com/traefik/whoami")

test.skip(!adminLink, "Zeabur Builder, a registry and Keel are not in this cluster")
test.use({ actionTimeout: 30_000 })

const name = `git-${Date.now().toString(36)}`
const whoami = repository === "https://github.com/traefik/whoami"

async function deploy(page: Page, service: string, branch = "") {
  await page.goto(`/projects/${encodeURIComponent(project)}`)
  await page.getByRole("button", { name: "New", exact: true }).click()
  await page.getByRole("menuitem", { name: "Deploy from Git", exact: true }).click()
  const drawer = page.getByRole("dialog", { name: "Deploy from Git" })
  await drawer.getByRole("textbox", { name: "Repository" }).fill(repository)
  const branchBox = drawer.getByRole("textbox", { name: "Branch" })
  await expect(branchBox).toHaveValue("")
  await expect(branchBox).toHaveAttribute("placeholder", "Default branch")
  if (branch) await branchBox.fill(branch)
  await drawer.getByRole("combobox", { name: "Build with" }).click()
  await page.getByRole("option", { name: "Dockerfile", exact: true }).click()
  await drawer.getByRole("textbox", { name: "Name" }).fill(service)
  await drawer.getByRole("textbox", { name: "Port" }).fill("80")
  await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
  const plan = page.getByRole("dialog", { name: `Deploy ${service} from Git` })
  await expect(plan.getByRole("button", { name: "Back" })).toBeVisible()
  await expect(plan).toContainText("Build")
  await plan.getByRole("button").last().click()
  await expect(plan).toHaveCount(0)
  await expect(toast(page, "Applied")).toBeVisible({ timeout: 30_000 })
}

const servicePath = (service: string) =>
  `/projects/${encodeURIComponent(project)}/services/Deployment/${service}`

test("a Developer deploys a public repository from Git", async ({ page, browser }) => {
  test.setTimeout(1_200_000)
  let developerLink = ""
  await test.step("the admin invites a Developer", async () => {
    await signInAsAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-git-developer", [{ project, role: "developer" }])
  })
  const developer = await member(browser, developerLink, "e2e-git-developer", zeabur)
  const made: string[] = []
  try {
    await test.step("a branch that does not exist fails at once, with the Builder's message", async () => {
      const broken = `${name}-nobranch`
      await deploy(developer, broken, "no-such-branch-e2e")
      made.push(broken)
      await developer.goto(servicePath(broken))
      const status = developer.locator("main header").first()
      await expect(status).toContainText("Build failed", { timeout: 120_000 })
      const main = developer.getByRole("main")
      const message = "no branch or tag `no-such-branch-e2e`"
      await expect(main).toContainText(message, { timeout: 60_000 })
      await main.getByRole("button", { name: "See the build" }).click()
      await expect(main).toContainText("Can't get the source")
      await expect(main).toContainText(message)
    })

    await test.step("the Developer fills in the repository and applies the plan", async () => {
      await deploy(developer, name)
      made.push(name)
    })

    const servicePage = servicePath(name)
    await test.step("the service is building, not failing to pull its image", async () => {
      await developer.goto(servicePage)
      const status = developer.locator("main header").first()
      await expect(status).toContainText(/Building|Ready/, { timeout: 60_000 })
      await expect(status).not.toContainText("Failed")
    })

    await test.step("the first image arrives and the service runs", async () => {
      await expect
        .poll(
          async () => {
            const answer = await developer.request.get(
              `/api/v1/projects/${encodeURIComponent(project)}`,
            )
            const body = (await answer.json()) as {
              services: { name: string; status: { word: string }; pods: { ready: boolean }[] }[]
            }
            const service = body.services.find((s) => s.name === name)
            expect(service?.status.word).not.toBe("failed")
            return !!service && service.pods.length > 0 && service.pods.every((p) => p.ready)
          },
          { timeout: 900_000, intervals: [10_000] },
        )
        .toBe(true)
      await developer.goto(`${servicePage}?tab=builds`)
      await expect(developer.getByRole("main")).toContainText("Built")
      if (whoami) await expect(developer.getByRole("main")).toContainText("whoami.git@master")
    })
  } finally {
    for (const service of made) {
      await test.step(`the Developer deletes ${service} and its Build`, () =>
        deleteService(developer, project, service))
    }
  }
})
