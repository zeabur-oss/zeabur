// End-to-end tests of the web interface against a running engawa (see e2e/ui.spec.ts).
//
//   ENGAWA_E2E_URL        the one-time launch link `engawactl ui` prints:
//                         http://127.0.0.1:<port>/?engawa-launch=<token>
//   ENGAWA_E2E_NAMESPACE  a namespace holding a Deployment named `talker` (e2e/manifests/talker.yaml)
//   ENGAWA_E2E_POD        one of talker's pods, which logs lines containing `engawa-e2e-line`
//
// The launch link works once: the `setup` project opens it, keeps the session cookie it sets in
// e2e/.auth/state.json, and every other test starts from that state.
import { defineConfig, devices } from "@playwright/test"

const launch = process.env.ENGAWA_E2E_URL
if (!launch) throw new Error("ENGAWA_E2E_URL is not set")

export default defineConfig({
  testDir: "e2e",
  // e2e/l2 runs against engawa in a cluster, with its own configuration (playwright.l2.config.ts).
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
