import { mkdir, mkdtemp, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"

import { expect, test } from "../fixtures.ts"
import { deleteService } from "./git.ts"
import { invite, member, signInAdmin, toast } from "./members.ts"

// Upload a folder: a Developer picks a small folder with a Dockerfile, applies the plan, and the
// folder goes up through Zeabur to the registry as an OCI artifact; Zeabur Builder builds it,
// Keel moves the service to the image and it runs. What `.gitignore` leaves out is not built.
// scripts/e2e-l2-browser.sh sets ZEABUR_L2_UPLOAD_ADMIN_LINK (an admin account's invitation)
// only when Zeabur Builder, a registry and Keel run here.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const adminLink = env("ZEABUR_L2_UPLOAD_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const zeabur = env("ZEABUR_L2_URL")

test.skip(!adminLink, "Zeabur Builder, a registry and Keel are not in this cluster")
test.use({ actionTimeout: 30_000 })

const name = `upload-${Date.now().toString(36)}`

/** A folder with whoami on port 80, a file `.gitignore` leaves out, and the `.gitignore`. */
async function folder() {
  const root = await mkdtemp(join(tmpdir(), "zeabur-upload-"))
  const site = join(root, name)
  await mkdir(join(site, "ignored"), { recursive: true })
  await writeFile(join(site, "Dockerfile"), "FROM docker.io/traefik/whoami:v1.10\n")
  await writeFile(join(site, ".gitignore"), "ignored/\n")
  await writeFile(join(site, "ignored", "big.bin"), "not built")
  return site
}

test("a Developer uploads a folder and the service runs it", async ({ page, browser }) => {
  test.setTimeout(1_200_000)
  let developerLink = ""
  await test.step("the admin invites a Developer", async () => {
    await signInAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-upload-developer", [{ project, role: "developer" }])
  })
  const developer = await member(browser, developerLink, "e2e-upload-developer", zeabur)
  try {
    await test.step("the Developer chooses the folder and applies the plan", async () => {
      await developer.goto(`/projects/${encodeURIComponent(project)}`)
      await developer.getByRole("button", { name: "New", exact: true }).click()
      await developer.getByRole("menuitem", { name: "Upload a folder", exact: true }).click()
      const drawer = developer.getByRole("dialog", { name: "Upload a folder" })
      await drawer
        .locator("input[type=file][webkitdirectory]")
        .setInputFiles(await folder())
      await expect(drawer).toContainText("2 files")
      await expect(drawer).toContainText("1 file left out")
      await expect(drawer.getByRole("textbox", { name: "Name" })).toHaveValue(name)
      await drawer.getByRole("combobox", { name: "Build with" }).click()
      await developer.getByRole("option", { name: "Dockerfile", exact: true }).click()
      await drawer.getByRole("textbox", { name: "Port" }).fill("80")
      await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
      const plan = developer.getByRole("dialog", { name: `Deploy ${name} from a folder` })
      await expect(plan).toContainText("Build")
      await plan.getByRole("button", { name: "Create", exact: true }).click()
      await expect(toast(developer, "Uploaded")).toBeVisible({ timeout: 120_000 })
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
      await developer.goto(
        `/projects/${encodeURIComponent(project)}/services/Deployment/${name}?tab=builds`,
      )
      const main = developer.getByRole("main")
      await expect(main).toContainText("Built")
      await expect(main).toContainText("Uploaded folder")
    })
  } finally {
    await test.step(`the Developer deletes ${name} and its Build`, () =>
      deleteService(developer, project, name))
  }
})
