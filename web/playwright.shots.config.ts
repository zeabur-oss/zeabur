// Documentation screenshots (e2e/shots/), run by scripts/docs-shots.sh against the demo server:
//   ZEABUR_SHOTS_URL       the demo's origin, http://127.0.0.1:<port>
//   ZEABUR_SHOTS_MANIFEST  docs/shots.yaml as JSON
//   ZEABUR_SHOTS_ENV       the demo's environment (e2e/shots/manifest.ts): only its shots run
//   ZEABUR_SHOTS_OUT       receives <locale>/<theme>/<id>.png
import { defineConfig, devices } from "@playwright/test"
import { locales, themes } from "./e2e/shots/manifest.ts"

const url = process.env.ZEABUR_SHOTS_URL
if (!url) throw new Error("ZEABUR_SHOTS_URL is not set")

export default defineConfig({
  testDir: "e2e/shots",
  testMatch: "shots.ts",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "list" : "line",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: url,
    // Full Chromium in its new headless mode, as in playwright.config.ts.
    channel: "chromium",
    // ZEABUR_SHOTS_SCALE: the README images use 1.
    deviceScaleFactor: Number(process.env.ZEABUR_SHOTS_SCALE ?? 2),
    reducedMotion: "reduce",
    timezoneId: "UTC",
    trace: "retain-on-failure",
    // GPU rasterisation changes a few pixels from run to run.
    launchOptions: { args: ["--disable-gpu", "--force-color-profile=srgb"] },
  },
  projects: Object.entries(locales).flatMap(([locale, tag]) =>
    themes.map((theme) => ({
      name: `${locale}/${theme}`,
      use: { locale: tag, colorScheme: theme },
    })),
  ),
})
