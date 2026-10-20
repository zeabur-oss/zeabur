// End-to-end tests against a running Zeabur (see e2e/ui.spec.ts).
//   ZEABUR_E2E_URL        the one-time launch link `zeaburctl ui` prints:
//                         http://127.0.0.1:<port>/?zeabur-launch=<token>
//   ZEABUR_E2E_NAMESPACE  a namespace holding a Deployment named `talker` (e2e/manifests/talker.yaml)
//   ZEABUR_E2E_POD        one of talker's pods, which logs lines containing `zeabur-e2e-line`
//
// The launch link works once: `setup` opens it and saves the session in e2e/.auth/state.json.
import { defineConfig, devices } from "@playwright/test"

const launch = process.env.ZEABUR_E2E_URL
if (!launch) throw new Error("ZEABUR_E2E_URL is not set")

export default defineConfig({
  testDir: "e2e",
  // Run by playwright.l2.config.ts.
  testIgnore: "**/e2e/l2/**",
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "list" : "line",
  use: {
    baseURL: new URL(launch).origin,
    locale: "en-US",
    // Full Chromium in its new headless mode: nixpkgs' Chromium-only browser set
    // (playwright-driver.browsers-chromium) has no separate headless shell.
    channel: "chromium",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "setup", testMatch: /launch\.setup\.ts/ },
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], storageState: "e2e/.auth/state.json" },
      dependencies: ["setup"],
    },
  ],
})
