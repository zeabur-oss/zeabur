import { expect, test as base } from "@playwright/test"

// Every test's page is watched for Content Security Policy violations from its first script on,
// and a test that saw one fails: a page that injects a style or a script the policy refuses
// breaks in the browser while every assertion about its text still passes. Fixtures marked
// `auto` run for every test (https://playwright.dev/docs/test-fixtures#automatic-fixtures).
export const test = base.extend<{ cspViolations: string[] }>({
  cspViolations: [
    async ({ page }, use) => {
      const seen: string[] = []
      await page.exposeFunction("reportCspViolation", (text: string) => seen.push(text))
      await page.addInitScript(() => {
        document.addEventListener("securitypolicyviolation", (event) => {
          const report = (window as unknown as { reportCspViolation: (text: string) => void })
            .reportCspViolation
          report(`${event.effectiveDirective} ${event.blockedURI} ${event.sample}`)
        })
      })
      page.on("console", (message) => {
        if (message.type() === "error" && /Content Security Policy/.test(message.text())) {
          seen.push(message.text())
        }
      })
      await use(seen)
      expect(seen, "Content Security Policy violations").toEqual([])
    },
    { auto: true },
  ],
})

export { expect }
