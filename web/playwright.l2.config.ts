// End-to-end tests of engawa in a cluster with Genkan (e2e/l2/), run by scripts/e2e-l2.sh:
//
//   ENGAWA_L2_URL   engawa's public URL, https://engawa.localhost
//   ENGAWA_L2_SPKI  base64 SHA-256 of the test certificate's public key (SubjectPublicKeyInfo)
//
// Both host names end in `.localhost` and reach the cluster's ingress on this machine's port 443
// (`--host-resolver-rules`). The certificate comes from a CA the script makes; naming its key in
// `--ignore-certificate-errors-spki-list` makes Chromium accept it as valid, which WebAuthn needs:
// Chromium refuses passkeys on a page with a certificate error, so `ignoreHTTPSErrors` would not
// do. Chromium honours that switch only with `--user-data-dir`, which Playwright always passes.
import { defineConfig, devices } from "@playwright/test"

const url = process.env.ENGAWA_L2_URL
if (!url) throw new Error("ENGAWA_L2_URL is not set")
const spki = process.env.ENGAWA_L2_SPKI
if (!spki) throw new Error("ENGAWA_L2_SPKI is not set")

export default defineConfig({
  testDir: "e2e/l2",
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "list" : "line",
  use: {
    baseURL: url,
    locale: "en-US",
    // Full Chromium in its new headless mode, as in playwright.config.ts.
    channel: "chromium",
    trace: "retain-on-failure",
    launchOptions: {
      args: [
        "--host-resolver-rules=MAP *.localhost 127.0.0.1",
        `--ignore-certificate-errors-spki-list=${spki}`,
      ],
    },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
})
