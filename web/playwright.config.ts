// End-to-end tests of the web interface against a running zeabur (see e2e/ui.spec.ts).
//
//   ZEABUR_E2E_URL        the one-time launch link `zeaburctl ui` prints:
//                         http://127.0.0.1:<port>/?zeabur-launch=<token>
//   ZEABUR_E2E_NAMESPACE  a namespace holding a Deployment named `talker` (e2e/manifests/talker.yaml)
//   ZEABUR_E2E_POD        one of talker's pods, which logs lines containing `zeabur-e2e-line`
//
// The launch link works once: the `setup` project opens it, keeps the session cookie it sets in
// e2e/.auth/state.json, and every other test starts from that state.
import { defineConfig, devices } from "@playwright/test"

const launch = process.env.ZEABUR_E2E_URL
if (!launch) throw new Error("ZEABUR_E2E_URL is not set")

export default defineConfig({
  testDir: "e2e",
  // e2e/l2 runs against Zeabur in a cluster, with its own configuration (playwright.l2.config.ts).
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
