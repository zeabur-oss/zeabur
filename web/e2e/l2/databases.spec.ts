import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { invite, member, planAndApply, signInAsAdmin, toast } from "./members.ts"

// Databases by role: a Developer creates a database, backs it up to S3 and backs it up now; a
// Viewer of the same project sees it but cannot back it up or read its password.
// scripts/e2e-l2-browser.sh sets, besides ZEABUR_L2_URL and ZEABUR_L2_NAMESPACE:
//
//   ZEABUR_L2_DB_ADMIN_LINK   an invitation for `e2e-db-admin`, in the groups admins and elevated
//                             admins (links work once, so not the roles test's)
//   ZEABUR_L2_S3_ENDPOINT     an S3 endpoint the cluster reaches, with the bucket `backups` and
//                             the keys below; set only when CloudNativePG runs in the cluster
//   ZEABUR_L2_MONGODB         1 when the Percona Operator for MongoDB runs in the cluster too
//   ZEABUR_L2_DB_KEEP         1 keeps the databases afterwards, to look at them
//   ZEABUR_L2_SERVICE         the Deployment each database is connected to
const env = (name: string, fallback = "") => process.env[name] ?? fallback
const keep = env("ZEABUR_L2_DB_KEEP") === "1"
const adminLink = env("ZEABUR_L2_DB_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const s3 = env("ZEABUR_L2_S3_ENDPOINT")
const mongodb = env("ZEABUR_L2_MONGODB") === "1"
const zeabur = env("ZEABUR_L2_URL")
const service = env("ZEABUR_L2_SERVICE", "talker")
const keys = {
  access: env("ZEABUR_L2_S3_ACCESS_KEY", "testkey"),
  secret: env("ZEABUR_L2_S3_SECRET_KEY", "testsecret123"),
}

test.skip(!s3, "CloudNativePG and a test S3 are not in this cluster")
// The test waits minutes for databases, never for a control.
test.use({ actionTimeout: 30_000 })

const suffix = Date.now().toString(36)
const postgres = `pg-${suffix}`
const mongo = `mongo-${suffix}`
const engines = [
  {
    engine: "PostgreSQL",
    path: "postgresql",
    name: postgres,
    kind: "Cluster",
    secret: `${postgres}-app`,
    variables: ["DATABASE_URL"],
    resource: { group: "postgresql.cnpg.io", version: "v1", resource: "clusters" },
    backup: {
      apiVersion: "postgresql.cnpg.io/v1",
      kind: "Backup",
      metadata: { name: `${postgres}-viewer` },
      spec: { cluster: { name: postgres } },
    },
  },
  ...(mongodb
    ? [
        {
          engine: "MongoDB",
          path: "mongodb",
          name: mongo,
          kind: "PerconaServerMongoDB",
          secret: `${mongo}-secrets`,
          variables: ["MONGOPASSWORD", "MONGO_URL"],
          resource: {
            group: "psmdb.percona.com",
            version: "v1",
            resource: "perconaservermongodbs",
          },
          backup: {
            apiVersion: "psmdb.percona.com/v1",
            kind: "PerconaServerMongoDBBackup",
            metadata: { name: `${mongo}-viewer` },
            spec: { clusterName: mongo, storageName: "backups" },
          },
        },
      ]
    : []),
]
const databasePage = (path: string, name: string) =>
  `/projects/${encodeURIComponent(project)}/databases/${path}/${name}`

/** The project's claims whose names start with `prefix`. */
async function claimsOf(page: Page, prefix: string): Promise<string[]> {
  const response = await page.request.get(
    `/api/v1/projects/${encodeURIComponent(project)}/objects?kinds=core/persistentvolumeclaims`,
  )
  expect(response.status()).toBe(200)
  const list = (await response.json()) as {
    items: { resource: string; group: string; items: { name: string }[] }[]
  }
  return list.items
    .filter((k) => k.group === "" && k.resource === "persistentvolumeclaims")
    .flatMap((k) => k.items.map((i) => i.name))
    .filter((name) => name.startsWith(prefix))
}

/** Confirms the plan dialog titled `title` with its last button, which names the change. A
 * drawer's plan step loads first with only Close, so its Back (a dialog's Cancel) comes first. */
async function apply(page: Page, title: string) {
  const dialog = page.getByRole("dialog", { name: title })
  await expect(dialog.getByRole("button", { name: /^(Back|Cancel)$/ })).toBeVisible()
  await dialog.getByRole("button").last().click()
  await expect(dialog).toHaveCount(0)
  await expect(toast(page, "Applied")).toBeVisible({ timeout: 30_000 })
}

test("a Developer creates and backs up databases; a Viewer only sees them", async ({
  page,
  browser,
}) => {
  test.setTimeout(engines.length * 900_000)
  let developerLink = ""
  let viewerLink = ""

  await test.step("the admin invites a Developer and a Viewer of the project", async () => {
    await signInAsAdmin(page, adminLink)
    developerLink = await invite(page, "e2e-db-developer", [{ project, role: "developer" }])
    viewerLink = await invite(page, "e2e-db-viewer", [{ project, role: "viewer" }])
  })

  const developer = await member(browser, developerLink, "e2e-db-developer", zeabur)
  for (const db of engines) {
    await test.step(`the Developer creates ${db.engine} database ${db.name}`, async () => {
      await developer.goto(`/projects/${encodeURIComponent(project)}`)
      await developer.getByRole("button", { name: "New", exact: true }).click()
      await developer.getByRole("menuitem", { name: db.engine, exact: true }).click()
      const drawer = developer.getByRole("dialog", { name: "New database" })
      await drawer.getByRole("textbox", { name: "Name" }).fill(db.name)
      await drawer.getByRole("textbox", { name: "Storage per instance" }).fill("1Gi")
      await expect(drawer.getByRole("alert")).toContainText("nothing takes over")
      await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
      await apply(developer, `Create database ${db.name}`)

      await developer.goto(databasePage(db.path, db.name))
      await expect(developer.getByRole("heading", { level: 1, name: db.name })).toBeVisible()
      // The first run pulls the database image.
      await expect(developer.locator("main header").first()).toContainText("Ready", {
        timeout: 600_000,
      })
    })

    await test.step(`the Developer connects ${db.name} to ${service}`, async () => {
      const connect = developer.getByRole("region", { name: "Connect" })
      await connect.getByRole("button", { name: "Connect to a service" }).click()
      const drawer = developer.getByRole("dialog", { name: `Connect ${db.name} to services` })
      await drawer.getByRole("checkbox", { name: service }).click()
      await expect(drawer).toContainText(db.variables.join(", "))
      await drawer.getByRole("button", { name: "Review changes", exact: true }).click()
      const title = `Connect ${db.name} to ${service}`
      // The plan is the drawer's second step; it names the restart and writes no password.
      const plan = developer.getByRole("dialog", { name: title })
      await expect(plan.getByRole("button", { name: "Back" })).toBeVisible()
      await expect(plan).toContainText(service)
      await apply(developer, title)
      await expect(connect).toContainText(`Connected to ${service}`, { timeout: 30_000 })

      await developer.goto(
        `/projects/${encodeURIComponent(project)}/services/Deployment/${service}?tab=variables`,
      )
      await expect(developer.getByRole("main")).toContainText(`From database ${db.name}`)
      // The new replicas start only once the Secret the variables read exists.
      await expect
        .poll(
          async () => {
            const answer = await developer.request.get(
              `/api/v1/projects/${encodeURIComponent(project)}`,
            )
            const body = (await answer.json()) as {
              services: { name: string; pods: { ready: boolean }[] }[]
            }
            const pods = body.services.find((s) => s.name === service)?.pods ?? []
            // Old replicas leave once the new ones are ready; a new one stuck on the Secret stays.
            return pods.length > 0 && pods.every((pod) => pod.ready)
          },
          { timeout: 180_000 },
        )
        .toBe(true)
      await developer.goto(databasePage(db.path, db.name))
    })

    await test.step(`the Developer reads ${db.name} as YAML and has a terminal`, async () => {
      await expect(developer.getByRole("button", { name: "Terminal", exact: true })).toBeEnabled()
      await developer.getByRole("tab", { name: "Logs" }).click()
      await expect(developer.getByRole("tabpanel")).toBeVisible()
      await developer.getByRole("tab", { name: "Settings" }).click()
      await developer
        .getByRole("group", { name: "Views" })
        .getByRole("button", { name: "YAML" })
        .click()
      const yaml = developer.getByRole("figure", { name: `YAML of ${db.name}` })
      await expect(yaml).toContainText(`kind: ${db.kind}`)
      await expect(developer.locator("main")).toContainText("kubectl exec")
      await developer.goto(databasePage(db.path, db.name))
    })

    await test.step(`the Developer backs ${db.name} up to S3 and backs it up now`, async () => {
      await developer.getByRole("tab", { name: "Backups" }).click()
      const setUp = developer.getByRole("region", { name: "Set up backups" })
      await setUp.getByRole("textbox", { name: "Endpoint" }).fill(s3)
      await setUp.getByRole("textbox", { name: "Bucket" }).fill("backups")
      await setUp.getByRole("textbox", { name: "Access key ID" }).fill(keys.access)
      await setUp.getByRole("textbox", { name: "Secret access key" }).fill(keys.secret)
      await setUp.getByRole("button", { name: "Review changes", exact: true }).click()
      const plan = developer.getByRole("dialog", { name: `Back up ${db.name}` })
      await expect(plan).not.toContainText(keys.secret)
      await apply(developer, `Back up ${db.name}`)
      const destination = developer.getByRole("region", { name: "Destination" })
      await expect(destination).toContainText(`s3://backups/${db.name}`, { timeout: 60_000 })

      // The operator restarts the members with backups first.
      const now = developer.getByRole("button", { name: "Back up now" })
      await expect(now).toBeEnabled({ timeout: 600_000 })
      await now.click()
      await apply(developer, `Back up ${db.name} now`)
      const runs = developer.getByRole("region", { name: "Backups" })
      await expect(runs.getByRole("listitem").first()).toContainText("Succeeded", {
        timeout: 600_000,
      })
      await developer.getByRole("tab", { name: "Overview" }).click()
      await expect(developer.getByRole("main")).not.toContainText("No backup yet")
    })
  }

  await test.step("the Viewer sees the databases but cannot back them up or read passwords", async () => {
    const viewer = await member(browser, viewerLink, "e2e-db-viewer", zeabur)
    for (const db of engines) {
      await viewer.goto(databasePage(db.path, db.name))
      await expect(viewer.locator("main header").first()).toContainText("Ready", {
        timeout: 60_000,
      })
      const connect = viewer.getByRole("region", { name: "Connect" })
      // Closed, naming the role that opens it, rather than refused after a click.
      await expect(connect.getByRole("button", { name: "Reveal" })).toBeDisabled()
      await expect(connect).toContainText("Needs Developer access")
      // Viewers do not read logs or open terminals.
      await expect(viewer.getByRole("tab", { name: "Logs" })).toHaveCount(0)
      await expect(viewer.getByRole("button", { name: "Terminal", exact: true })).toBeDisabled()
      await viewer.getByRole("tab", { name: "Backups" }).click()
      const backups = viewer.getByRole("region", { name: "Backups" })
      await expect(backups).toContainText("Succeeded")
      await expect(viewer.getByRole("button", { name: "Back up now" })).toBeDisabled()

      const refused = await planAndApply(viewer, project, [{ op: "create", object: db.backup }])
      expect(refused.status, JSON.stringify(refused.body)).toBe(403)
    }
    await viewer.context().close()
  })

  await test.step("the Developer disconnects and deletes the databases", async () => {
    for (const db of keep ? [] : engines) {
      const disconnected = await planAndApply(developer, project, [
        {
          op: "env",
          workload: { kind: "Deployment", name: service },
          changes: db.variables.map((name) => ({ op: "remove", name })),
        },
      ])
      expect(disconnected.status, JSON.stringify(disconnected.body)).toBe(200)
      const deleted = await planAndApply(developer, project, [
        { op: "delete", resource: db.resource, name: db.name },
      ])
      expect(deleted.status, JSON.stringify(deleted.body)).toBe(200)
      if (db.path === "mongodb")
        await expect
          .poll(() => claimsOf(developer, `mongod-data-${db.name}-`), { timeout: 300_000 })
          .toEqual([])
    }
    await developer.context().close()
  })
})
