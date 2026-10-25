import type { Page } from "@playwright/test"

import { expect, test } from "../fixtures.ts"
import { invite, member, planAndApply, signInAsAdmin } from "./members.ts"

// kubectl terminals: every tools pod runs in zeabur-terminals, where members have no rights. A
// Developer opens their project's terminal and cannot see, list, delete or open an admin's tools
// pod; the admin's terminal for the same project works. scripts/e2e-l2-browser.sh sets, besides
// ZEABUR_L2_URL:
//
//   ZEABUR_L2_TERMINAL_ADMIN_LINK  an invitation for `e2e-terminal-admin`, in the groups admins and
//                                  elevated admins
//   ZEABUR_L2_NAMESPACE            the Developer's project
const env = (name: string) => process.env[name] ?? ""
const adminLink = env("ZEABUR_L2_TERMINAL_ADMIN_LINK")
const project = env("ZEABUR_L2_NAMESPACE")
const zeabur = env("ZEABUR_L2_URL")
const tools = `/api/v1/projects/${encodeURIComponent(project)}/tools`

type Tools = { namespace: string; pods: { name: string; ready: boolean }[] }

/** Plans and applies the project's tools pod; its name. */
async function startTools(page: Page): Promise<string> {
  const applied = await planAndApply(page, project, [{ op: "clusterTools" }])
  expect(applied.status, JSON.stringify(applied)).toBe(200)
  let name = ""
  await expect
    .poll(
      async () => {
        const status = (await (await page.request.get(tools)).json()) as Tools
        expect(status.namespace).toBe("zeabur-terminals")
        const pod = status.pods[0]
        name = pod?.name ?? ""
        return pod?.ready ?? false
      },
      { timeout: 180_000, intervals: [2_000] },
    )
    .toBe(true)
  return name
}

/** Opens the terminal of tools pod `name`, types `input`, and returns the first control frame and
 * what the terminal showed until `input` ran. The terminal stays open, so the pod stays. */
async function terminal(page: Page, name: string, input: string) {
  return page.evaluate(
    async ({ path, input }) =>
      new Promise<{ first: Record<string, unknown>; shown: string }>((resolve) => {
        const url = new URL(path, location.href)
        url.protocol = url.protocol.replace("http", "ws")
        const socket = new WebSocket(url)
        socket.binaryType = "arraybuffer"
        const decoder = new TextDecoder()
        let first: Record<string, unknown> = {}
        let shown = ""
        const done = () => resolve({ first, shown })
        setTimeout(done, 60_000)
        socket.onmessage = (event) => {
          if (typeof event.data === "string") {
            const frame = JSON.parse(event.data) as Record<string, unknown>
            if (Object.keys(first).length === 0) first = frame
            if (frame.type === "ready") {
              socket.send(new TextEncoder().encode(`${input}; echo END-$((1+1))\r`))
            }
          } else {
            shown += decoder.decode(event.data as ArrayBuffer, { stream: true })
            if (shown.includes("END-2")) done()
          }
        }
        socket.onclose = done
      }),
    { path: `${tools}/${encodeURIComponent(name)}/terminal`, input },
  )
}

test.beforeAll(() => {
  if (!adminLink) throw new Error("ZEABUR_L2_TERMINAL_ADMIN_LINK is not set")
  if (!project) throw new Error("ZEABUR_L2_NAMESPACE is not set")
})

test("a Developer's kubectl terminal is theirs alone, and an admin's works beside it", async ({
  page,
  browser,
}) => {
  test.setTimeout(600_000)
  let developer: Page
  let adminPod = ""

  await test.step("the admin opens a kubectl terminal for the project", async () => {
    await signInAsAdmin(page, adminLink)
    adminPod = await startTools(page)
    const { first, shown } = await terminal(
      page,
      adminPod,
      "kubectl auth whoami -o jsonpath='{.status.userInfo.username}'; echo; " +
        "kubectl config view --minify -o jsonpath='{..namespace}'; echo",
    )
    expect(first).toEqual({ type: "ready", container: "tools" })
    expect(shown).toContain("zeabur:e2e-terminal-admin")
    expect(shown).toContain(project)
  })

  await test.step("the Developer opens their own, which acts as them in the project", async () => {
    const link = await invite(page, "e2e-terminal-developer", [{ project, role: "developer" }])
    developer = await member(browser, link, "e2e-terminal-developer", zeabur)
    const mine = await startTools(developer)
    expect(mine).not.toBe(adminPod)
    const { first, shown } = await terminal(
      developer,
      mine,
      "kubectl auth whoami -o jsonpath='{.status.userInfo.username}'; echo; " +
        "kubectl get deployments -o name; " +
        "kubectl get pods -n zeabur-terminals; " +
        `kubectl exec -n zeabur-terminals ${adminPod} -- true`,
    )
    expect(first).toEqual({ type: "ready", container: "tools" })
    expect(shown).toContain("zeabur:e2e-terminal-developer")
    expect(shown).toContain("deployment.apps/talker")
    expect(shown).toMatch(/cannot list resource "pods".*"zeabur-terminals"/)
    expect(shown).toMatch(/cannot (get|create) resource "pods.*"zeabur-terminals"/)
  })

  await test.step("Zeabur shows the Developer only their own pod, and not the admin's", async () => {
    const status = (await (await developer.request.get(tools)).json()) as Tools
    expect(status.pods.map((p) => p.name)).not.toContain(adminPod)
    const deleted = await developer.evaluate(async (path) => {
      const response = await fetch(path, {
        method: "DELETE",
        credentials: "same-origin",
        headers: { "zeabur-request": "1" },
      })
      return { status: response.status, body: await response.text() }
    }, `${tools}/${adminPod}`)
    expect(deleted.status, deleted.body).toBe(404)
    const { first } = await terminal(developer, adminPod, "true")
    expect(first.type).toBe("error")
    const admin = (await (await page.request.get(tools)).json()) as Tools
    expect(admin.pods.map((p) => p.name)).toContain(adminPod)
  })
})
