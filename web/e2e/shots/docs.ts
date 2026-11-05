// The screenshots of the documentation and oss.zeabur.com (sets.ts), from the in-memory demo
// (crates/zeabur-api/examples/demo.rs), or with `app: "installer"` from the installer's pages and
// responses built from the pinned catalog (crates/zeabur-installer/web/scripts/dev-api.ts). Each
// test is one image, named by its title. Kept in git in each locale's docs folder, light only:
// docs/images/shots/<id>.avif for English, docs/zh-TW/images/shots/<id>.avif for a translation. A
// page links its locale's image by a relative path, such as ../images/shots/<id>.avif.
// Selectors never use visible text: it changes with the locale.
import type { Page } from "@playwright/test"
import { clickUntil, expect, test } from "./fixtures.ts"

const next = (page: Page) => page.locator('[data-testid="step-next"] button:last-child')
const toStep = (page: Page, step: string) =>
  clickUntil(next(page), page.locator(`main[data-step="${step}"]`))

test.describe(() => {
  test.use({ app: "installer" })

  // The installer (docs/zh-TW/get-started/quickstart.mdx). Each step's last button continues.
  test("installer-host", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await expect(page.locator('[data-testid="step-next"]')).toBeVisible()
    await shot()
  })
  test("installer-distribution", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await expect(page.locator('section:has(p ~ [role="radiogroup"])')).toBeVisible()
    await shot({ clip: 'section:has(p ~ [role="radiogroup"])' })
  })
  test("installer-access", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await toStep(page, "access")
    await expect(page.locator('[data-slot="field"] .text-ok-strong').first()).toBeVisible()
    await expect(
      page.locator('[data-slot="field"] ~ [data-slot="field"] .text-ok-strong'),
    ).toBeVisible()
    await shot()
  })
  test("installer-components", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await toStep(page, "components")
    await expect(page.locator('[data-slot="checkbox"][data-checked]').first()).toBeVisible()
    await shot()
  })
  test("installer-review", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await toStep(page, "review")
    await expect(page.locator('[role="alert"]')).toBeVisible()
    await shot()
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_INSTALL: "running" } })
    // Stopped before Zeabur: the folded timeline then shows five items.
    test("installer-install", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await toStep(page, "review")
      await next(page).click()
      await expect(page.locator('[data-slot="timeline-item"]:nth-of-type(5)')).toBeVisible()
      await shot()
    })
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_INSTALL: "done" } })
    test("installer-done", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await toStep(page, "review")
      await next(page).click()
      await expect(page.locator('[data-slot="copy-input"]').first()).toBeVisible()
      await shot()
    })
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_INSTALL: "fail" } })
    test("installer-failed", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await toStep(page, "review")
      await next(page).click()
      await expect(page.locator('[data-slot="timeline-item"]').first()).toBeVisible()
      await expect(page.locator('[role="alert"]')).toBeVisible()
      await shot()
    })
  })

  // Personal install (docs/zh-TW/install/personal.mdx). ZEABUR_DEMO_HOST: small has 1.5 GiB, docker has Docker.
  test("installer-personal-choose", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await page.locator('[data-slot="radio-option"]').first().click()
    await expect(page.locator('[data-slot="radio-option"][data-checked]')).toBeVisible()
    await shot()
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_HOST: "small" } })
    test("installer-personal-small", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await expect(page.locator('[data-slot="radio-option"][data-checked]')).toBeVisible()
      await shot()
    })
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_HOST: "docker" } })
    test("installer-personal-docker", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await page.locator('[data-slot="radio-option"]').first().click()
      await expect(page.locator('[data-slot="radio-option"][data-checked]')).toBeVisible()
      await shot()
    })
  })
  test("installer-personal-components", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await page.locator('[data-slot="radio-option"]').first().click()
    await toStep(page, "components")
    await page
      .locator('[data-slot="checkbox"]:not([data-checked]):not([data-disabled])')
      .first()
      .click()
    await expect(
      page.locator('[data-slot="checkbox"][data-disabled][data-checked]').first(),
    ).toBeVisible()
    await shot()
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_HOST: "small" } })
    test("installer-personal-components-small", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await toStep(page, "components")
      await expect(page.locator('[data-slot="checkbox"][data-checked]').first()).toBeVisible()
      await shot()
    })
  })
  test.describe(() => {
    test.use({ demo: { ZEABUR_DEMO_INSTALL: "done" } })
    test("installer-personal-done", async ({ page, shot }) => {
      await page.goto("/", { waitUntil: "networkidle" })
      await page.locator('[data-slot="radio-option"]').first().click()
      await toStep(page, "review")
      await next(page).click()
      await expect(page.locator('[data-slot="copy-input"]').first()).toBeVisible()
      await shot()
    })
  })
})

// Zeabur pages, from the demo.
test("git-builds", async ({ page, shot }) => {
  await page.goto("/projects/blog/services/Deployment/docs?tab=builds", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("service-overview", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("project-services", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("existing-namespaces", async ({ page, shot }) => {
  await page.goto("/projects", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("database-overview", async ({ page, shot }) => {
  await page.goto("/projects/shop/databases/postgresql/orders", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("object-storage", async ({ page, shot }) => {
  await page.goto("/projects/lab/object-storage/files", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("cron-job", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/CronJob/nightly-report", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("service-variables", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=variables", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("service-storage", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/StatefulSet/redis?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("section#storage")).toBeVisible()
  await shot({ clip: "section#storage" })
})
test("service-networking", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("#net-public")).toBeVisible()
  await shot({ clip: "main" })
})
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_RKE2: "1" } })
  test("cluster-nodes", async ({ page, shot }) => {
    await page.goto("/cluster", { waitUntil: "networkidle" })
    await expect(page.locator("#cluster-nodes")).toBeVisible()
    await shot({ clip: "#cluster-nodes" })
  })
})
test("cluster-components", async ({ page, shot }) => {
  await page.goto("/cluster/components", { waitUntil: "networkidle" })
  await expect(page.locator("#cluster-installed")).toBeVisible()
  await shot({
    clip: "main",
    until: '[aria-labelledby="components-catalogue"] section:nth-of-type(2)',
  })
})
// Notification channels (docs/zh-TW/cluster/notifications.mdx).
test("notify-channel-add", async ({ page, shot }) => {
  await page.goto("/cluster/notifications", { waitUntil: "networkidle" })
  await expect(page.locator('main [data-slot="card"]')).toBeVisible()
  await page.locator('main div:has(> p[role="status"]) button').click()
  await page.locator('[data-slot="dialog-content"] input[maxlength="30"]').fill("ops-slack")
  await page
    .locator('[data-slot="dialog-content"] input[type="password"]')
    .fill("https://hooks.slack.com/services/T0000/B0000/XXXXXXXX")
  await page.locator('[data-slot="dialog-content"] label[for$="-node-not-ready"]').click()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("notify-channels", async ({ page, shot }) => {
  await page.goto("/cluster/notifications", { waitUntil: "networkidle" })
  await expect(page.locator('main [data-slot="card"]')).toBeVisible()
  await shot({ clip: 'main [data-slot="card"]' })
})
// Inviting a member (docs/zh-TW/cluster/members.mdx).
test("members-invite", async ({ page, shot }) => {
  await page.goto("/members", { waitUntil: "networkidle" })
  await page.locator('[data-slot="page-header"] button').click()
  await page.locator('[data-slot="dialog-content"] input[translate="no"]').fill("erin")
  await page
    .locator('[data-slot="dialog-content"] input[data-slot="input"]:not([translate]):not([type])')
    .fill("Erin Huang")
  await page.locator('[data-slot="dialog-content"] label[for$="-blog"]').click()
  await page.locator('[data-slot="dialog-content"] input[type="search"]').fill("shop")
  await page.locator('[data-slot="dialog-content"] label[for$="-shop"]').click()
  await page
    .locator(
      '[data-slot="dialog-content"] li:has(label[for$="-shop"]) [data-slot="select-trigger"]',
    )
    .click()
  await page.locator('[role="option"]:not([aria-selected="true"])').first().click()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("members-invite-review", async ({ page, shot }) => {
  await page.goto("/members", { waitUntil: "networkidle" })
  await page.locator('[data-slot="page-header"] button').click()
  await page.locator('[data-slot="dialog-content"] input[translate="no"]').fill("erin")
  await page.locator('[data-slot="dialog-content"] label[for$="-shop"]').click()
  await page
    .locator(
      '[data-slot="dialog-content"] li:has(label[for$="-shop"]) [data-slot="select-trigger"]',
    )
    .click()
  await page.locator('[role="option"]:not([aria-selected="true"])').first().click()
  await page.locator('[data-slot="dialog-content"] > div:last-child button:last-child').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test.describe(() => {
  test.use({ fresh: true })
  test("members-invite-link", async ({ page, shot }) => {
    await page.goto("/members", { waitUntil: "networkidle" })
    await page.locator('[data-slot="page-header"] button').click()
    await page.locator('[data-slot="dialog-content"] input[translate="no"]').fill("erin")
    await page.locator('[data-slot="dialog-content"] label[for$="-shop"]').click()
    await page.locator('[data-slot="dialog-content"] > div:last-child button:last-child').click()
    await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
    await page.locator('[data-slot="dialog-content"] > div:last-child button:last-child').click()
    await expect(
      page.locator('[data-slot="dialog-content"] [data-slot="copy-input"]'),
    ).toBeVisible()
    await shot({
      clip: '[data-slot="dialog-content"]',
      blank: '[data-slot="dialog-content"] [data-slot="copy-input"] input',
    })
  })
})
// S3 destinations and cluster backups (docs/zh-TW/cluster/s3-destinations.mdx, backups.mdx).
test("s3-destination-add", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#s3-destinations")).toBeVisible()
  await page.locator("#s3-destinations > div:first-child button").click()
  await page.locator("#s3-destinations form > :first-child input").fill("b2-archive")
  await page
    .locator("#s3-destinations form .grid > :nth-child(1) input")
    .fill("https://s3.us-west-004.backblazeb2.com")
  await page.locator("#s3-destinations form .grid > :nth-child(2) input").fill("acme-archive")
  await page.locator("#s3-destinations form .grid > :nth-child(3) input").fill("zeabur")
  await page.locator("#s3-destinations form .grid > :nth-child(4) input").fill("us-west-004")
  await page
    .locator("#s3-destinations form .grid > :nth-child(5) input")
    .fill("004a1b2c3d4e5f6000000000a")
  await page.locator('#s3-destinations form input[type="password"]').fill("K004demo-secret-key")
  await shot({ clip: "#s3-destinations form" })
})
test("s3-destination-keys", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#s3-destinations li")).toBeVisible()
  await page.locator("#s3-destinations li button").first().click()
  await page
    .locator("#s3-destinations form .grid > :nth-child(5) input")
    .fill("7d1f0c9e2b4a6c8e0f1a3b5c7d9e1f20")
  await page.locator('#s3-destinations form input[type="password"]').fill("demo-new-secret-key")
  await page.locator('#s3-destinations form button[type="submit"]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("backup-location-form", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-location")).toBeVisible()
  await page.locator("#backup-location > div:first-child button").click()
  await page.locator('#backup-location [data-slot="select-trigger"]').click()
  await page.locator('[role="option"]:not([aria-selected="true"])').first().click()
  await shot({ clip: "#backup-location" })
})
test("backup-location", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-location")).toBeVisible()
  await shot({ clip: "#backup-location" })
})
test("backup-schedule-form", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-schedule")).toBeVisible()
  await page.locator("#backup-schedule > div:first-child button:last-child").click()
  await shot({ clip: "#backup-schedule" })
})
test("backup-list", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-list")).toBeVisible()
  await shot({ clip: "#backup-list" })
})
test("backup-restore", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-list")).toBeVisible()
  await page.locator("#backup-list li:nth-child(3) button[aria-expanded]").first().click()
  await page.locator('#backup-list li:has(form) [data-slot="select-trigger"]').click()
  await page.locator('[role="option"]:not([aria-selected="true"])').first().click()
  await shot({ clip: "#backup-list li:has(form)" })
})
test("backup-restore-deleted", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-list")).toBeVisible()
  await page.locator("#backup-list li:nth-child(3) button[aria-expanded]").first().click()
  await expect(page.locator('#backup-list li form[aria-busy="false"]')).toBeVisible()
  await page.locator('#backup-list li:has(form) [data-slot="select-trigger"]').click()
  await page.locator('[role="option"]:last-child').click()
  await shot({ clip: "#backup-list li:has(form)" })
})
test("backup-restore-plan", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#backup-list")).toBeVisible()
  await page.locator("#backup-list li:nth-child(3) button[aria-expanded]").first().click()
  await page.locator('#backup-list li:has(form) [data-slot="select-trigger"]').click()
  await page.locator('[role="option"]:not([aria-selected="true"])').first().click()
  await page.locator('#backup-list li:has(form) button[type="submit"]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("backup-attach", async ({ page, shot }) => {
  await page.goto("/cluster/backups", { waitUntil: "networkidle" })
  await expect(page.locator("#restore-list")).toBeVisible()
  await page.locator('#restore-list [data-testid="attach-domains"]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// Maintenance mode of one domain (docs/zh-TW/configure/maintenance.mdx).
test("domain-pages-open", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator('[data-testid="domain-pages"]').first()).toBeVisible()
  await page.locator('[data-testid="domain-pages"]').first().hover()
  await shot({ clip: 'section[aria-labelledby="net-public"]', keepPointer: true })
})
test("domain-pages-maintenance", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator('[data-testid="domain-pages"]').first()).toBeVisible()
  await page.locator('[data-testid="domain-pages"]').first().click()
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("domain-pages-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator('[data-testid="domain-pages"]').first()).toBeVisible()
  await page.locator('[data-testid="domain-pages"]').first().click()
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await page.locator('[data-slot="dialog-footer"] button:last-child').click()
  await page.locator('[data-slot="draft-bar"] button:last-child').click()
  await expect(
    page.locator('[data-slot="dialog-content"]:has-text("shop.example.com")'),
  ).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test.describe(() => {
  test.use({ fresh: true })
  test("domain-maintenance-on", async ({ page, shot }) => {
    await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
      waitUntil: "networkidle",
    })
    await expect(page.locator('[data-testid="domain-pages"]').first()).toBeVisible()
    await page.locator('[data-testid="domain-pages"]').first().click()
    await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
    await page.locator('[data-slot="dialog-footer"] button:last-child').click()
    await page.locator('[data-slot="draft-bar"] button:last-child').click()
    await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
    await page.locator('[data-slot="dialog-content"] > div:last-child button:last-child').click()
    await expect(page.locator('[data-testid="in-maintenance"]')).toBeVisible()
    await shot({ clip: 'section[aria-labelledby="net-public"]' })
  })
})
// First project and an image (docs/zh-TW/get-started/first-project.mdx, deploy/image.mdx).
test("project-new", async ({ page, shot }) => {
  await page.goto("/projects", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="new-project"]')).toBeVisible()
  await page.locator('[data-testid="new-project"]').click()
  await page.locator('[data-slot="dialog-content"] input').first().fill("hello")
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("project-new-review", async ({ page, shot }) => {
  await page.goto("/projects", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="new-project"]')).toBeVisible()
  await page.locator('[data-testid="new-project"]').click()
  await page.locator('[data-slot="dialog-content"] input').first().fill("hello")
  await page.locator('[data-slot="dialog-content"] button[type="submit"]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]:has(button:text-is("YAML"))' })
})
test("image-form", async ({ page, shot }) => {
  await page.goto("/projects/edge", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-image"]').click()
  await page
    .locator('[data-slot="dialog-content"] input')
    .first()
    .fill("docker.io/nginxinc/nginx-unprivileged:1.30.5-alpine-slim")
  await page.locator('[data-slot="dialog-content"] input').nth(1).fill("web")
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await expect(
    page.locator('[data-slot="dialog-content"] input[value$="apps.example.com"]'),
  ).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("image-review", async ({ page, shot }) => {
  await page.goto("/projects/edge", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-image"]').click()
  await page
    .locator('[data-slot="dialog-content"] input')
    .first()
    .fill("docker.io/nginxinc/nginx-unprivileged:1.30.5-alpine-slim")
  await page.locator('[data-slot="dialog-content"] input').nth(1).fill("web")
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await expect(
    page.locator('[data-slot="dialog-content"] input[value$="apps.example.com"]'),
  ).toBeVisible()
  await page.locator('[data-slot="dialog-footer"] button:last-child').click()
  await expect(
    page.locator('[data-slot="dialog-body"] > :not([hidden]) button:text-is("YAML")'),
  ).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// Git (docs/zh-TW/deploy/git.mdx); Keel makes Deploy from Git available.
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_KEEL: "1" } })
  test("git-form", async ({ page, shot }) => {
    await page.goto("/projects/edge", { waitUntil: "networkidle" })
    await expect(page.locator('[data-testid="add"]')).toBeVisible()
    await page.locator('[data-testid="add"]').click()
    await page.locator('[data-testid="add-git"]').click()
    await page
      .locator('[data-slot="dialog-content"] input')
      .first()
      .fill("https://github.com/acme/web")
    await page.locator('[data-slot="dialog-content"] input').nth(3).fill("web")
    await shot({ clip: '[data-slot="dialog-content"]' })
  })
})
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_KEEL: "1", ZEABUR_DEMO_GITHUB: "installed" } })
  test("git-github", async ({ page, shot }) => {
    await page.goto("/projects/shop", { waitUntil: "networkidle" })
    await expect(page.locator('[data-testid="add"]')).toBeVisible()
    await page.locator('[data-testid="add"]').click()
    await page.locator('[data-testid="add-git"]').click()
    await page.locator('[data-slot="dialog-content"] [data-slot="combobox-input"]').click()
    await page.locator('[data-slot="combobox-item"]').first().click()
    await expect(
      page.locator('[data-slot="dialog-content"] input[placeholder="main"]'),
    ).toBeVisible()
    await shot({ clip: '[data-slot="dialog-content"]' })
  })
})
// Compose (docs/zh-TW/deploy/compose.mdx).
test("compose-services", async ({ page, shot }) => {
  await page.goto("/projects/edge", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-compose"]').click()
  await page
    .locator('[data-slot="dialog-content"] textarea')
    .fill(
      'services:\n  web:\n    image: ghcr.io/acme/web:1.4\n    ports: ["3000:3000"]\n    environment:\n      DATABASE_URL: postgres://app:secret@db:5432/app\n    depends_on: [db]\n  db:\n    image: postgres:17\n    environment:\n      POSTGRES_USER: app\n      POSTGRES_PASSWORD: secret\n      POSTGRES_DB: app\n    volumes: [pgdata:/var/lib/postgresql/data]\nvolumes:\n  pgdata: {}\n',
    )
  await page.locator('[data-slot="dialog-footer"] button:last-child').click()
  await expect(
    page.locator('[data-slot="dialog-content"] [data-slot="section-label"]'),
  ).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"] div:has(> [data-slot="section-label"])' })
})
// Databases and object storage (docs/zh-TW/deploy/databases.mdx, object-storage.mdx).
test("database-form", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-postgresql"]').click()
  await page.locator('[data-slot="dialog-content"] [data-slot="checkbox"]').nth(2).click()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("object-storage-form", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator('[data-testid="add"]')).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-object-storage"]').click()
  await page.locator('[data-slot="dialog-content"] [data-slot="checkbox"]').nth(2).click()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// A service's settings (docs/zh-TW/deploy/cron-jobs.mdx, configure/variables.mdx, volumes.mdx, domains.mdx).
test("cron-job-form", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("#tasks button")).toBeVisible()
  await page.locator("#tasks button").click()
  await page.locator('[data-slot="dialog-content"] input').first().fill("npm run cleanup")
  await page.locator('[data-slot="dialog-content"] input').nth(2).fill("Asia/Taipei")
  await page.locator('[data-slot="dialog-content"] input').nth(3).fill("nightly-cleanup")
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("variables-edit", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=variables", {
    waitUntil: "networkidle",
  })
  await expect(page.locator('main [data-slot="section-header"]')).toBeVisible()
  await page
    .locator('main [data-slot="section-header"] [role="group"] ~ button:last-of-type')
    .click()
  await expect(page.locator('main [data-slot="tabs-content"] input').first()).toBeVisible()
  await shot({ clip: 'main [data-slot="tabs-content"]' })
})
test("volume-add", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("#storage button").first()).toBeVisible()
  await page.locator("#storage button").first().click()
  await page.locator("#storage input").first().fill("/data")
  await page.locator("#storage input").nth(1).fill("5Gi")
  await shot({ clip: "#storage" })
})
test("domain-add", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(
    page.locator('section[aria-labelledby="net-public"] [data-slot="section-header"] button'),
  ).toBeVisible()
  await page
    .locator('section[aria-labelledby="net-public"] [data-slot="section-header"] button')
    .click()
  await page
    .locator('section[aria-labelledby="net-public"] form input')
    .first()
    .fill("app.example.com")
  await shot({ clip: 'section[aria-labelledby="net-public"]' })
})
test("domain-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(
    page.locator('section[aria-labelledby="net-public"] [data-slot="section-header"] button'),
  ).toBeVisible()
  await page
    .locator('section[aria-labelledby="net-public"] [data-slot="section-header"] button')
    .click()
  await page
    .locator('section[aria-labelledby="net-public"] form input')
    .first()
    .fill("app.example.com")
  await page.locator('section[aria-labelledby="net-public"] form button[type="submit"]').click()
  await page.locator('[data-slot="draft-bar"] button:last-child').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})

// Feature pages: what the reader chooses or checks (forms, review dialogs, results).
// Deploy from an app template (docs/zh-TW/deploy/templates.mdx).
test("template-list", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-template"]').click()
  await page.locator('[data-slot="dialog-content"] input[type="search"]').fill("n8n")
  await expect(page.locator('[data-testid="template-n8n-sqlite"]')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("template-form", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-template"]').click()
  await page.locator('[data-testid="template-miniflux"]').click()
  await expect(page.locator('[data-slot="dialog-content"] form')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("template-review", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('[data-testid="add"]').click()
  await page.locator('[data-testid="add-template"]').click()
  await page.locator('[data-testid="template-miniflux"]').click()
  await expect(page.locator('[data-slot="dialog-content"] form')).toBeVisible()
  await page.locator('[data-slot="dialog-footer"] button[type="submit"]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// Upload a folder (docs/zh-TW/deploy/upload.mdx); Keel makes it available.
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_KEEL: "1" } })
  test("upload-form", async ({ page, shot }) => {
    await page.goto("/projects/shop", { waitUntil: "networkidle" })
    await expect(page.locator("main h1")).toBeVisible()
    await page.locator('[data-testid="add"]').click()
    await page.locator('[data-testid="add-upload"]').click()
    await expect(page.locator('[data-slot="dialog-content"] button').first()).toBeVisible()
    await shot({ clip: '[data-slot="dialog-content"]' })
  })
})
// A service's resources, probes, security, releases, logs and terminal (docs/zh-TW/configure/service-settings.mdx, restart-rollback.mdx, logs-terminal.mdx).
test("service-compute", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "section#compute" })
})
test("service-health", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "section#health-checks" })
})
test("service-security", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=settings", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "section#security" })
})
test("restart-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator("main button").nth(1).click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("release-compare", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=deployments", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await expect(page.locator('[data-slot="timeline-item"]:has(button)').first()).toBeVisible()
  await page.locator('[data-slot="timeline-item"]').nth(2).locator("button").first().click()
  await shot({ clip: ':nth-match([data-slot="timeline-item"], 3)' })
})
test("rollback-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=deployments", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await expect(page.locator('[data-slot="timeline-item"]:has(button)').first()).toBeVisible()
  await page.locator('[data-slot="timeline-item"]').nth(2).locator("button:last-child").click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("service-logs", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=logs", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("service-usage", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=usage", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("terminal-connected", async ({ page, shot }) => {
  await page.goto("/projects/lab/services/Deployment/talker", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator("main button").nth(3).click()
  await expect(page.locator("#dock .xterm")).toBeVisible()
  await page.keyboard.press("l")
  await page.keyboard.press("s")
  await page.keyboard.press("Enter")
  await shot({ clip: "body" })
})
test("terminal-no-shell", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator("main button").nth(3).click()
  await expect(
    page.locator("#dock button[data-slot=button]:not([aria-label])").first(),
  ).toBeVisible()
  await shot({ clip: "#dock" })
})
// YAML objects (docs/zh-TW/configure/objects-yaml.mdx).
test("objects-create", async ({ page, shot }) => {
  await page.goto("/projects/shop/create", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page
    .locator(".cm-content")
    .fill(
      "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: hello-config\ndata:\n  MESSAGE: hello\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: hello\n  labels:\n    app.kubernetes.io/name: hello\nspec:\n  replicas: 1\n  selector:\n    matchLabels:\n      app.kubernetes.io/name: hello\n  template:\n    metadata:\n      labels:\n        app.kubernetes.io/name: hello\n    spec:\n      automountServiceAccountToken: false\n      securityContext:\n        seccompProfile:\n          type: RuntimeDefault\n      containers:\n        - name: hello\n          image: nginxinc/nginx-unprivileged:1.29\n          ports:\n            - containerPort: 8080\n          envFrom:\n            - configMapRef:\n                name: hello-config\n          securityContext:\n            allowPrivilegeEscalation: false\n---\napiVersion: v1\nkind: Service\nmetadata:\n  name: hello\n  labels:\n    app.kubernetes.io/name: hello\nspec:\n  selector:\n    app.kubernetes.io/name: hello\n  ports:\n    - name: http\n      port: 8080\n      targetPort: 8080\n",
    )
  await shot({ clip: "main" })
})
test("objects-create-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/create", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page
    .locator(".cm-content")
    .fill(
      "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: hello-config\ndata:\n  MESSAGE: hello\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: hello\n  labels:\n    app.kubernetes.io/name: hello\nspec:\n  replicas: 1\n  selector:\n    matchLabels:\n      app.kubernetes.io/name: hello\n  template:\n    metadata:\n      labels:\n        app.kubernetes.io/name: hello\n    spec:\n      automountServiceAccountToken: false\n      securityContext:\n        seccompProfile:\n          type: RuntimeDefault\n      containers:\n        - name: hello\n          image: nginxinc/nginx-unprivileged:1.29\n          ports:\n            - containerPort: 8080\n          envFrom:\n            - configMapRef:\n                name: hello-config\n          securityContext:\n            allowPrivilegeEscalation: false\n---\napiVersion: v1\nkind: Service\nmetadata:\n  name: hello\n  labels:\n    app.kubernetes.io/name: hello\nspec:\n  selector:\n    app.kubernetes.io/name: hello\n  ports:\n    - name: http\n      port: 8080\n      targetPort: 8080\n",
    )
  await page.locator('xpath=(//main//button[@data-slot="button"])[last()]').click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// DNS, IP limits and deleting a project (docs/zh-TW/configure/dns.mdx, ip-limits.mdx, delete-project.mdx).
test("cluster-domains", async ({ page, shot }) => {
  await page.goto("/cluster", { waitUntil: "networkidle" })
  await expect(page.locator("#cluster-domains")).toBeVisible()
  await shot({ clip: "#cluster-domains" })
})
test("ip-limit-form", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('section[aria-labelledby="net-ip"] li:first-child button').click()
  await expect(page.locator('[data-slot="dialog-content"]')).toBeVisible()
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await page.locator('[data-slot="dialog-content"] textarea').fill("203.0.113.0/24\n2001:db8::/32")
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("ip-limit-out-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=networking", {
    waitUntil: "networkidle",
  })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('section[aria-labelledby="net-ip"] li:last-child button').click()
  await expect(page.locator('[data-slot="dialog-content"]')).toBeVisible()
  await page.locator('[data-slot="dialog-content"] [data-slot="switch"]').first().click()
  await page.locator('[data-slot="dialog-content"] textarea').fill("203.0.113.10\n198.51.100.0/24")
  await page
    .locator(
      '[data-slot="dialog-content"] button[type="submit"], [data-slot="dialog-footer"] button:last-child',
    )
    .click()
  await expect(page.locator('[data-slot="dialog-content"] button:text-is("YAML")')).toBeVisible()
  await shot({ clip: '[data-slot="dialog-content"]' })
})
test("project-delete-review", async ({ page, shot }) => {
  await page.goto("/projects/blog/settings", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('xpath=(//main//button[@data-slot="button"])[last()]').click()
  await expect(page.locator('[data-slot="dialog-content"]')).toBeVisible()
  await page.locator('[data-slot="dialog-content"] input').first().fill("blog")
  await shot({ clip: '[data-slot="dialog-content"]' })
})
// Cluster settings and diagnosis (docs/zh-TW/cluster/settings.mdx, diagnosis.mdx).
test("cluster-settings", async ({ page, shot }) => {
  await page.goto("/settings", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("home-problems", async ({ page, shot }) => {
  await page.goto("/", { waitUntil: "networkidle" })
  await expect(page.locator("#home-problems")).toBeVisible()
  await expect(page.locator("#home-projects")).toBeVisible()
  await shot({ clip: "main", until: "section:has(#home-problems)" })
})
test("service-problem", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/worker", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("service-connectivity", async ({ page, shot }) => {
  await page.goto("/projects/lab/services/Deployment/talker", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('main [data-slot="collapsible-trigger"]').click()
  await shot({ clip: "main" })
})
test("cluster-attention", async ({ page, shot }) => {
  await page.goto("/cluster", { waitUntil: "networkidle" })
  await expect(page.locator("#cluster-attention-title")).toBeVisible()
  await shot({ clip: "section:has(#cluster-attention-title)" })
})
// GitHub App, made by the demo's fake GitHub (docs/zh-TW/cluster/github-app.mdx); `installed` also installs it.
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_GITHUB: "1" } })
  test("github-app-create", async ({ page, shot }) => {
    await page.goto("/cluster/github-app", { waitUntil: "networkidle" })
    await expect(page.locator("#github-app")).toBeVisible()
    await page.locator("#github-app button").first().click()
    await shot({ clip: "#github-app" })
  })
})
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_GITHUB: "installed" } })
  test("github-app-installed", async ({ page, shot }) => {
    await page.goto("/cluster/github-app", { waitUntil: "networkidle" })
    await expect(page.locator("#github-app")).toBeVisible()
    await shot({ clip: "#github-app" })
  })
})
test.describe(() => {
  test.use({ demo: { ZEABUR_DEMO_GITHUB: "installed" } })
  test("github-app-grant", async ({ page, shot }) => {
    await page.goto("/cluster/github-app", { waitUntil: "networkidle" })
    await expect(page.locator("#github-app")).toBeVisible()
    await page.locator("#github-app li button").first().click()
    await shot({ clip: "#github-app" })
  })
})
// External Secrets (docs/zh-TW/cluster/external-secrets.mdx): the demo's ExternalSecret `db`, synced from OpenBao.
test("external-secret-synced", async ({ page, shot }) => {
  await page.goto("/projects/shop/objects", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await page.locator('main input[type="search"], main input').fill("external")
  await page.locator('main button[class*="h-8.5"]').nth(0).click()
  await expect(page.locator('[role="dialog"]')).toBeVisible()
  await shot({ clip: '[role="dialog"]' })
})
// The landing page at oss.zeabur.com.
test("project", async ({ page, shot }) => {
  await page.goto("/projects/shop", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("diagnosis", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/worker", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("database", async ({ page, shot }) => {
  await page.goto("/projects/shop/databases/postgresql/orders", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main" })
})
test("cluster", async ({ page, shot }) => {
  await page.goto("/cluster", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main", until: "#cluster-nodes" })
})
test.describe(() => {
  test.use({ app: "installer" })
  test("installer", async ({ page, shot }) => {
    await page.goto("/", { waitUntil: "networkidle" })
    await toStep(page, "review")
    await expect(page.locator('[role="alert"]')).toBeVisible()
    await shot()
  })
})
// Change NODE_ENV, save, and open the technical details of the review.
test("change-review", async ({ page, shot }) => {
  await page.goto("/projects/shop/services/Deployment/web?tab=variables", {
    waitUntil: "networkidle",
  })
  await page
    .locator(
      '[data-slot="tabs-content"] [data-slot="section-header"] button[data-slot="button"]:nth-of-type(3)',
    )
    .click()
  await page.locator('[data-slot="tabs-content"] input[data-slot="input"]').nth(1).fill("staging")
  await page.locator('[data-slot="draft-bar"] button[data-slot="button"]:last-child').click()
  await page.locator('[role="dialog"] button:text-is("YAML")').click()
  await expect(page.locator('[role="dialog"] pre, [role="dialog"] code').first()).toBeVisible()
  await shot({ clip: '[role="dialog"]' })
})
