import type { Page } from "@playwright/test"

import { expect } from "../fixtures.ts"
import { toast } from "./members.ts"

/** Deletes the service from its page with what it made: its Build, its Kubernetes Service and
 * its `-env` Secret are gone after, and so are the objects in `also`, which the plan names. */
export async function deleteService(
  page: Page,
  project: string,
  service: string,
  { also = [] }: { also?: string[] } = {},
) {
  await page.goto(`/projects/${encodeURIComponent(project)}/services/Deployment/${service}`)
  await page
    .locator("main header")
    .first()
    .getByRole("button", { name: "More", exact: true })
    .click()
  await page.getByRole("menuitem", { name: "Delete service" }).click()
  const plan = page.getByRole("dialog", { name: `Delete Deployment ${service}` })
  await expect(plan).toContainText(`Delete ${service}. This can't be undone.`)
  for (const object of also) await expect(plan).toContainText(object)
  await plan.getByRole("textbox", { name: `Type ${service} to confirm` }).fill(service)
  await plan.getByRole("button").last().click()
  await expect(toast(page, "Applied")).toBeVisible({ timeout: 30_000 })
  await expect
    .poll(async () => {
      const answer = await page.request.get(
        `/api/v1/projects/${encodeURIComponent(project)}/builds`,
      )
      return ((await answer.json()) as { builds: { name: string }[] }).builds.map((b) => b.name)
    })
    .not.toContain(service)
  const left = (resources: { kind: string; name: string }[]) =>
    resources
      .map((r) => `${r.kind} ${r.name}`)
      .filter((r) => [`Service ${service}`, `Secret ${service}-env`, ...also].includes(r))
  await expect
    .poll(
      async () => {
        const answer = await page.request.get(`/api/v1/projects/${encodeURIComponent(project)}`)
        return left(
          ((await answer.json()) as { resources: { kind: string; name: string }[] }).resources,
        )
      },
      { timeout: 60_000 },
    )
    .toEqual([])
}
