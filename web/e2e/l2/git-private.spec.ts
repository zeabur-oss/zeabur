import { execFileSync } from "node:child_process"
import { mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { deleteService } from "./git.ts"
import { invite, member, signInAsAdmin, toast } from "./members.ts"

// Deploy from Git with a deploy key, against a real private repository on GitHub whose
// Dockerfile serves a page on port 8080. Until the key is on the repository the service fails
// with GitHub's refusal; once it is, the service builds and runs; when the key is taken off
// again the next poll fails and the service says so. Skipped unless E2E_PRIVATE_GIT_REPO is set.
//
//   E2E_PRIVATE_GIT_REPO   the repository, e.g. git@github.com:zeabur-oss/oss-private-git-demo.git
//   E2E_PRIVATE_GIT_TEXT   text its index.html holds
//   E2E_GH_DEPLOY_KEY=1    add the key (read-only) and remove it with `gh repo deploy-key`, as
//                          the account `gh` is signed in with; otherwise the test prints the key
//                          and waits for someone to add it, then to remove it
//
// scripts/e2e-l2-browser.sh passes E2E_PRIVATE_GIT_* through and sets
// ZEABUR_L2_GIT_PRIVATE_ADMIN_LINK when Zeabur Builder runs in the cluster.
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const repository = env("E2E_PRIVATE_GIT_REPO")
const text = env("E2E_PRIVATE_GIT_TEXT", "Built by Zeabur from a private repository.")
const useGh = env("E2E_GH_DEPLOY_KEY") === "1"
const adminLink = env("ZEABUR_L2_GIT_PRIVATE_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const zeabur = env("ZEABUR_L2_URL")

test.skip(!repository, "E2E_PRIVATE_GIT_REPO is not set")
test.skip(!adminLink, "Zeabur Builder, a registry and Keel are not in this cluster")
test.use({ actionTimeout: 30_000 })

const name = `git-private-${Date.now().toString(36)}`
const ghRepo = /github\.com[:/]([^/]+\/[^/]+?)(\.git)?$/.exec(repository)?.[1] ?? ""
const keyTitle = `zeabur-oss-e2e-${new Date().toISOString().slice(0, 10)}`
const servicePath = `/projects/${encodeURIComponent(project)}/services/Deployment/${name}`

const gh = (...args: string[]) => execFileSync("gh", args, { encoding: "utf8" })

/** The deploy keys on the repository that are `publicKey`, by id. */
function keyIds(publicKey: string): number[] {
  const blob = publicKey.split(" ")[1]
  const keys = JSON.parse(gh("repo", "deploy-key", "list", "-R", ghRepo, "--json", "id,key")) as {
    id: number
    key: string
  }[]
  return keys.filter((k) => k.key.split(" ")[1] === blob).map((k) => k.id)
}

async function addKey(publicKey: string) {
  if (!useGh) {
    console.log(`E2E_DEPLOY_KEY_ADD ${ghRepo} ${keyTitle} ${publicKey}`)
    return
  }
  const dir = mkdtempSync(join(tmpdir(), "zeabur-e2e-"))
  try {
    const file = join(dir, "key.pub")
    writeFileSync(file, `${publicKey}\n`)
    gh("repo", "deploy-key", "add", file, "-R", ghRepo, "-t", keyTitle)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

function removeKey(publicKey: string) {
  if (!useGh) {
    console.log(`E2E_DEPLOY_KEY_REMOVE ${ghRepo} ${publicKey}`)
    return
  }
  for (const id of keyIds(publicKey)) gh("repo", "deploy-key", "delete", String(id), "-R", ghRepo)
}

type ProjectView = {
  services: {
    name: string
    status: { word: string; reason?: string }
    pods: { name: string; ready: boolean }[]
  }[]
}

const serviceOf = async (page: Page) => {
  const answer = await page.request.get(`/api/v1/projects/${encodeURIComponent(project)}`)
  return ((await answer.json()) as ProjectView).services.find((s) => s.name === name)
}

test("a Developer deploys a private repository with a deploy key", async ({ page, browser }) => {
  test.setTimeout(1_800_000)
  let developerLink = ""
  await test.step("the admin invites a Developer", async () => {
    await signInAsAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-git-private-developer", [
      { project, role: "developer" },
    ])
  })
  const developer = await member(browser, developerLink, "e2e-git-private-developer", zeabur)

  let publicKey = ""
  await test.step("the Developer makes a deploy key and applies the plan", async () => {
    await developer.goto(`/projects/${encodeURIComponent(project)}`)
    await developer.getByRole("button", { name: "New", exact: true }).click()
    await developer.getByRole("menuitem", { name: "Deploy from Git", exact: true }).click()
    const drawer = developer.getByRole("dialog", { name: "Deploy from Git" })
    await drawer.getByRole("textbox", { name: "Repository" }).fill(repository)
    await drawer.getByRole("radio", { name: /^Deploy key/ }).click()
    await drawer.getByRole("button", { name: "Create deploy key" }).click()
    const key = drawer.getByRole("textbox", { name: "Public key" })
    await expect(key).toHaveValue(/^ssh-ed25519 \S+ zeabur /)
    publicKey = await key.inputValue()
    await expect(drawer).toContainText("without write access")
    await drawer.getByRole("combobox", { name: "Build with" }).click()
    await developer.getByRole("option", { name: "Dockerfile", exact: true }).click()
    await drawer.getByRole("textbox", { name: "Name" }).fill(name)
    await drawer.getByRole("textbox", { name: "Port" }).fill("8080")
    await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
    const plan = developer.getByRole("dialog", { name: `Deploy ${name} from Git` })
    await expect(plan).toContainText(`${name}-git`)
    await expect(plan).not.toContainText("PRIVATE KEY")
    await plan.getByRole("button").last().click()
    await expect(plan).toHaveCount(0)
    await expect(toast(developer, "Applied")).toBeVisible({ timeout: 30_000 })
  })

  try {
    await test.step("before the key is on the repository, the service fails with GitHub's refusal", async () => {
      await developer.goto(servicePath)
      const main = developer.getByRole("main")
      await expect(developer.locator("main header").first()).toContainText("Build failed", {
        timeout: 180_000,
      })
      await expect(main).toContainText(/refused the (deploy )?key/, { timeout: 60_000 })
      await main.getByRole("button", { name: "See the build" }).click()
      await expect(main).toContainText("Can't get the source")
    })

    await test.step("once the key is added, the service builds from main and runs", async () => {
      await addKey(publicKey)
      await expect
        .poll(
          async () => {
            const service = await serviceOf(developer)
            return !!service && service.pods.length > 0 && service.pods.every((p) => p.ready)
          },
          { timeout: 900_000, intervals: [10_000] },
        )
        .toBe(true)
      await developer.goto(`${servicePath}?tab=builds`)
      const main = developer.getByRole("main")
      await expect(main).toContainText("Built")
      await expect(main).toContainText(/oss-private-git-demo\.git@main|\.git@main/)
      const pod = (await serviceOf(developer))?.pods[0]?.name ?? ""
      const file = await developer.request.get(
        `/api/v1/projects/${encodeURIComponent(project)}/pods/${pod}/files/content?path=${encodeURIComponent("/www/index.html")}`,
      )
      expect(file.status()).toBe(200)
      expect(await file.text()).toContain(text)
    })

    await test.step("when the key is removed, the service says new commits cannot be fetched", async () => {
      removeKey(publicKey)
      await developer.goto(servicePath)
      const main = developer.getByRole("main")
      await expect(main).toContainText(/refused the (deploy )?key/, { timeout: 300_000 })
      const service = await serviceOf(developer)
      expect(service?.pods.every((p) => p.ready)).toBe(true)
    })
  } finally {
    if (useGh && publicKey) removeKey(publicKey)
    await deleteService(developer, project, name)
  }
})
