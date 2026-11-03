// Documentation screenshots (e2e/shots/), run by scripts/docs-shots.sh: one project for each set,
// locale and theme (e2e/shots/sets.ts), such as docs/en/light. The demo servers are fixtures
// (e2e/shots/fixtures.ts).
import { defineConfig, devices } from "@playwright/test"
import { locales, sets, type Locale } from "./e2e/shots/sets.ts"

export default defineConfig({
  testDir: "e2e/shots",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "list" : "line",
  // A wait in a shot may take as long as the test.
  expect: { timeout: 30_000 },
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    // Full Chromium in its new headless mode, as in playwright.config.ts.
    channel: "chromium",
    reducedMotion: "reduce",
    timezoneId: "UTC",
    trace: "retain-on-failure",
    // GPU rasterisation changes a few pixels from run to run.
    launchOptions: { args: ["--disable-gpu", "--force-color-profile=srgb"] },
  },
  projects: Object.entries(sets).flatMap(([name, set]) =>
    set.locales.flatMap((locale: Locale) =>
      set.themes.map((theme) => ({
        name: `${name}/${locale}/${theme}`,
        testMatch: `${name}.ts`,
        use: { locale: locales[locale], colorScheme: theme, deviceScaleFactor: set.scale },
      })),
    ),
  ),
})
