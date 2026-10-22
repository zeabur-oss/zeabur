import { readFile, writeFile } from "node:fs/promises"
import type { Page } from "@playwright/test"

import { expect, test } from "./fixtures.ts"

// The Files tab against e2e/manifests/files.yaml, which scripts/e2e-ui.sh applies.
const namespace = `${process.env.ZEABUR_E2E_NAMESPACE ?? "zeabur-e2e"}-files`
const project = `/projects/${encodeURIComponent(namespace)}`

const panel = (page: Page) => page.getByRole("tabpanel", { name: "Files" })
const volumes = (page: Page) => panel(page).getByRole("table", { name: "Volumes" })
const folder = (page: Page, path: string) => panel(page).getByRole("table", { name: path })

async function openFiles(page: Page, service: string) {
  await page.goto(`${project}/services/Deployment/${service}?tab=files`)
  await expect(volumes(page)).toBeVisible({ timeout: 30_000 })
}

async function openVolume(page: Page, path: string) {
  await volumes(page).getByRole("button", { name: path, exact: true }).click()
}

async function rowAction(page: Page, path: string, name: string, action: string) {
  await folder(page, path)
    .getByRole("button", { name: `Actions for ${name}` })
    .click()
  await page.getByRole("menuitem", { name: action }).click()
}

test("a claim's files are listed, previewed, downloaded and changed after a confirmation", async ({
  page,
}, info) => {
  await openFiles(page, "keeper")
  const rows = volumes(page)
  await expect(rows.getByRole("row", { name: /\/data/ })).toContainText("Claim keeper-data")
  await expect(rows.getByRole("row", { name: /\/etc\/keeper\b(?!-)/ })).toContainText("Read-only")
  await expect(rows.getByRole("row", { name: /\/scratch/ })).toContainText("Lost when replaced")

  await openVolume(page, "/data")
  const data = folder(page, "/data")
  await expect(data.getByRole("button", { name: "-odd name.txt", exact: true })).toBeVisible()

  await data.getByRole("button", { name: "hello.txt", exact: true }).click()
  const preview = page.getByRole("dialog", { name: "hello.txt" })
  await expect(preview).toContainText("zeabur-e2e-files")
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    preview.getByRole("link", { name: "Download" }).click(),
  ])
  expect(download.suggestedFilename()).toBe("hello.txt")
  expect(await readFile(await download.path(), "utf8")).toBe("zeabur-e2e-files\n")
  await preview.getByRole("button", { name: "Close", exact: true }).click()

  await panel(page).getByRole("button", { name: "New folder", exact: true }).click()
  const create = page.getByRole("dialog", { name: "New folder" })
  await create.getByRole("textbox", { name: "Folder name" }).fill("reports")
  await create.getByRole("button", { name: "Create", exact: true }).click()
  await expect(data.getByRole("button", { name: "reports", exact: true })).toBeVisible()

  await rowAction(page, "/data", "reports", "Rename")
  const rename = page.getByRole("dialog", { name: "Rename reports" })
  await rename.getByRole("textbox", { name: "New name" }).fill("reports 2024")
  await rename.getByRole("button", { name: "Rename", exact: true }).click()
  await expect(data.getByRole("button", { name: "reports 2024", exact: true })).toBeVisible()

  const upload = info.outputPath("--upload me.txt")
  await writeFile(upload, "uploaded\n")
  for (const replaced of [false, true]) {
    await panel(page).locator('input[type="file"]').setInputFiles(upload)
    const confirm = page.getByRole("dialog", { name: "Upload 1 file" })
    await expect(confirm).toContainText("--upload me.txt")
    await expect(confirm.getByText("Replaces", { exact: true })).toHaveCount(replaced ? 1 : 0)
    await confirm.getByRole("button", { name: "Upload", exact: true }).click()
    await expect(panel(page).getByText("Upload finished")).toBeVisible({ timeout: 30_000 })
    await expect(data.getByRole("button", { name: "--upload me.txt", exact: true })).toBeVisible()
  }

  await rowAction(page, "/data", "--upload me.txt", "Delete")
  const remove = page.getByRole("dialog", { name: "Delete --upload me.txt" })
  await expect(remove).toContainText("claim keeper-data")
  await remove.getByRole("button", { name: "Delete --upload me.txt", exact: true }).click()
  await expect(data.getByRole("button", { name: "--upload me.txt", exact: true })).toHaveCount(0)
})

test("a Secret's files show their names only, and their contents stay behind the reveal", async ({
  page,
}) => {
  await openFiles(page, "keeper")
  await openVolume(page, "/etc/keeper")
  const secret = folder(page, "/etc/keeper")
  await expect(secret.getByRole("row", { name: /token/ })).toContainText("hidden")
  await expect(secret.getByRole("button", { name: "token", exact: true })).toHaveCount(0)
  await expect(
    panel(page).getByRole("button", { name: "Reveal in keeper-secret", exact: true }),
  ).toBeVisible()
  await expect(panel(page).getByText("zeabur-e2e-files-secret")).toHaveCount(0)

  const document = (await (await page.request.get(`/api/v1/projects/${namespace}`)).json()) as {
    services: { name: string; pods: { name: string }[] }[]
  }
  const pod = document.services.find((s) => s.name === "keeper")?.pods[0]?.name
  expect(pod).toBeTruthy()
  for (const path of ["/etc/keeper/token", "/etc/keeper/..data/token"]) {
    const read = await page.request.get(
      `/api/v1/projects/${namespace}/pods/${pod}/files/content?path=${encodeURIComponent(path)}`,
    )
    expect(read.status()).toBe(409)
    expect(await read.text()).not.toContain("zeabur-e2e-files-secret")
  }
})

test("a distroless image's claim is read through a debug container", async ({ page }) => {
  await openFiles(page, "distroless")
  const offer = panel(page)
  await expect(offer).toContainText("This image has no tar.")
  await expect(volumes(page).getByRole("row", { name: /\/data/ })).toContainText("Needs tar")
  await offer.getByRole("button", { name: "Add a debug container", exact: true }).click()
  const dialog = page.getByRole("dialog", { name: "Add a debug container" })
  // The review names the op as the terminal does.
  await dialog.getByRole("button", { name: "Open debug terminal", exact: true }).click()
  await expect(offer).toContainText("read through debug container", { timeout: 60_000 })
  await expect(volumes(page).getByRole("row", { name: /^\/ / })).toContainText(
    "Not reachable from the debug container",
  )
  await openVolume(page, "/data")
  await expect(offer.getByText("Empty folder.")).toBeVisible()
})
