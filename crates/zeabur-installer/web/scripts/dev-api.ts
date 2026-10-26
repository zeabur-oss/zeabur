// The API for `bun run dev`. With INSTALLER_GUI (the address of a running `zeabur-installer gui`,
// tunnelled here) and INSTALLER_TOKEN (its printed token), reads go to that server, so the pages
// show a real host and catalog; without them, the captured responses in fixtures/ answer.
// Installing is always simulated: the log replays fixtures/apply.log (INSTALLER_FAIL=1, or an
// `installer_fail=1` cookie, stops it partway), so nothing changes on the server.
// `vite preview` serves the built pages with the same API, for scripts/docs-shots.sh. There,
// ZEABUR_DEMO_INSTALL gives the whole log at the first read: `done`, `running` (stops before
// Zeabur) or `fail`. ZEABUR_DEMO_HOST changes the server: `small` (1.5 GiB, so only a personal
// install fits and Zeabur Builder is out of reach) or `docker` (Docker found, which stops a
// personal install).
import { readFileSync } from "node:fs"
import type { IncomingMessage, ServerResponse } from "node:http"
import { resolve } from "node:path"
import type { Connect, Plugin } from "vite"

const fixtures = resolve(import.meta.dirname, "fixtures")
const fixture = (name: string) => readFileSync(resolve(fixtures, name), "utf8")
const backend = process.env.INSTALLER_GUI
const demo = process.env.ZEABUR_DEMO_INSTALL
const host = process.env.ZEABUR_DEMO_HOST
const token = process.env.INSTALLER_TOKEN ?? ""

async function forward(path: string, body?: string): Promise<string> {
  const response = await fetch(`${backend}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      Cookie: `zeabur_installer=${token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body,
  })
  return response.text()
}

function read(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    let body = ""
    request.on("data", (chunk: Buffer) => (body += chunk.toString()))
    request.on("end", () => done(body))
  })
}

let started = 0
const log = (fail: boolean, personal: boolean) => {
  const lines = fixture(personal ? "personal-apply.log" : "apply.log")
    .trimEnd()
    .split("\n")
  if (!fail) return lines
  const [after, waiting] = personal
    ? ["==> cert-manager", "deployment/cert-manager"]
    : ["==> Zeabur Identity", "deployment/zeabur-identity"]
  const stop = lines.findIndex((l) => l.startsWith(after)) + 2
  return [...lines.slice(0, stop), `error: timed out after 5m0s waiting for ${waiting}`]
}

const memoryMib = host === "small" ? 1536 : 7936
const buildsMinMib = 1800
const builder = "builds/zeabur-builder"

// The captured server has 7936 MiB; the plan and the state follow ZEABUR_DEMO_HOST.
function state(): string {
  const value = JSON.parse(fixture("state.json"))
  value.facts.memoryMib = memoryMib
  if (host === "docker") {
    const found = ["/usr/bin/docker", "docker.service"]
    value.personalChecks = value.personalChecks.map((check: { name: string }) =>
      check.name === "docker"
        ? {
            name: "docker",
            status: "fail",
            code: "docker.found",
            detail: `Docker is on this host (${found.join(", ")}). It sets the iptables FORWARD policy to DROP, so Pods cannot reach the internet and certificates fail. Stop and remove Docker first`,
            values: found,
          }
        : check,
    )
  }
  return JSON.stringify(value)
}

function personalPlan(body: string): string {
  const config = JSON.parse(body || "{}").config ?? {}
  const wanted =
    (config.enable ?? []).includes(builder) &&
    !(config.disable ?? []).includes(builder) &&
    memoryMib >= buildsMinMib
  const value = JSON.parse(fixture(wanted ? "personal-builds-plan.json" : "personal-plan.json"))
  value.plan.facts.memoryMib = memoryMib
  value.plan.memory.hostMib = memoryMib
  value.text = value.text
    .replace(/ of 7936 /, ` of ${memoryMib} `)
    .replace(/\(7936 MiB\)/, `(${memoryMib} MiB)`)
  if (memoryMib < buildsMinMib && (config.enable ?? []).includes(builder)) {
    value.plan.leftOut[builder] = `needs ${buildsMinMib} MiB of memory`
  }
  return JSON.stringify(value)
}

async function answer(request: IncomingMessage, response: ServerResponse) {
  const url = new URL(request.url ?? "/", "http://localhost")
  const fail =
    demo === "fail" ||
    process.env.INSTALLER_FAIL === "1" ||
    /\binstaller_fail=1\b/.test(request.headers.cookie ?? "")
  const json = (text: string) => {
    response.setHeader("Content-Type", "application/json")
    response.end(text)
  }
  if (url.pathname === "/api/apply" && request.method === "POST") {
    // The kind of install is the visitor's own, as `fail` is: shots of both kinds share a server.
    const kind = (await read(request)).includes('"use":"personal"') ? "personal" : "team"
    response.setHeader("Set-Cookie", `installer_use=${kind}; Path=/; SameSite=Strict`)
    started = Date.now()
    return json(JSON.stringify({ started: true }))
  }
  if (url.pathname === "/api/apply") {
    const personal = /\binstaller_use=personal\b/.test(request.headers.cookie ?? "")
    const all = log(fail, personal)
    const stopAt = personal ? "==> Done" : "==> Zeabur"
    const lines = demo === "running" ? all.slice(0, all.indexOf(stopAt)) : all
    // One line every 120 ms, so a run takes about as long as reading it.
    const paced = Math.min(lines.length, Math.floor((Date.now() - started) / 120))
    const shown = started ? (demo ? lines.length : paced) : 0
    const from = Number(url.searchParams.get("from") ?? 0)
    const running = started > 0 && (demo === "running" || shown < lines.length)
    const code = started && !running ? (fail ? 1 : 0) : null
    return json(
      JSON.stringify({
        lines: lines.slice(from, shown),
        next: Math.max(from, shown),
        running,
        code,
      }),
    )
  }
  if (url.pathname === "/api/local-ca.crt") {
    response.setHeader("Content-Type", "application/x-x509-ca-cert")
    return response.end("-----BEGIN CERTIFICATE-----\n…\n-----END CERTIFICATE-----\n")
  }
  if (backend) {
    const body = request.method === "POST" ? await read(request) : undefined
    return json(await forward(`${url.pathname}${url.search}`, body))
  }
  if (url.pathname === "/api/state") return json(state())
  if (url.pathname === "/api/plan") {
    const body = request.method === "POST" ? await read(request) : ""
    return json(body.includes('"use":"personal"') ? personalPlan(body) : fixture("plan.json"))
  }
  if (url.pathname === "/api/resolve") {
    const name = url.searchParams.get("name") ?? ""
    const address =
      /\.(\d+\.\d+\.\d+\.\d+)\.sslip\.io$/.exec(name)?.[1] ??
      /\.(\d+-\d+-\d+-\d+)\.sslip\.io$/.exec(name)?.[1]?.replaceAll("-", ".")
    return json(JSON.stringify({ name, addresses: address ? [address] : [] }))
  }
  response.statusCode = 404
  response.end("not found")
}

const middleware: Connect.NextHandleFunction = (request, response, next) => {
  if (!request.url?.startsWith("/api/")) return next()
  answer(request, response).catch((error: unknown) => {
    response.statusCode = 502
    response.end(String(error))
  })
}

export const devApi: Plugin = {
  name: "installer-dev-api",
  configureServer: (server) => void server.middlewares.use(middleware),
  configurePreviewServer: (server) => void server.middlewares.use(middleware),
}
