import { expect, test } from "../fixtures.ts"
import { invite, member, signInAsAdmin, toast } from "./members.ts"

// Deploy from Git: a Developer deploys a small public repository with its Dockerfile; the service
// is building until Zeabur Builder pushes the first image, then Keel moves it there and it runs.
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

test("a Developer deploys a public repository from Git", async ({ page, browser }) => {
  test.setTimeout(1_200_000)
  let developerLink = ""
  await test.step("the admin invites a Developer", async () => {
    await signInAsAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-git-developer", [{ project, role: "developer" }])
  })
  const developer = await member(browser, developerLink, "e2e-git-developer", zeabur)

  await test.step("the Developer fills in the repository and applies the plan", async () => {
    await developer.goto(`/projects/${encodeURIComponent(project)}`)
    await developer.getByRole("button", { name: "New", exact: true }).click()
    await developer.getByRole("menuitem", { name: "Deploy from Git", exact: true }).click()
    const drawer = developer.getByRole("dialog", { name: "Deploy from Git" })
    await drawer.getByRole("textbox", { name: "Repository" }).fill(repository)
    await drawer.getByRole("combobox", { name: "Build with" }).click()
    await developer.getByRole("option", { name: "Dockerfile", exact: true }).click()
    await drawer.getByRole("textbox", { name: "Name" }).fill(name)
    await drawer.getByRole("textbox", { name: "Port" }).fill("80")
    await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
    const plan = developer.getByRole("dialog", { name: `Deploy ${name} from Git` })
    await expect(plan.getByRole("button", { name: "Back" })).toBeVisible()
    await expect(plan).toContainText("Build")
    await plan.getByRole("button").last().click()
    await expect(plan).toHaveCount(0)
    await expect(toast(developer, "Applied")).toBeVisible({ timeout: 30_000 })
  })

  const servicePage = `/projects/${encodeURIComponent(project)}/services/Deployment/${name}`
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
  })
})
