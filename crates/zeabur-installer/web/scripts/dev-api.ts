// The API for `bun run dev`. With INSTALLER_GUI (the address of a running `zeabur-installer gui`,
// tunnelled here) and INSTALLER_TOKEN (its printed token), reads go to that server, so the pages
// show a real host and catalog; without them, the captured responses in fixtures/ answer.
// Installing is always simulated: the log replays fixtures/apply.log (INSTALLER_FAIL=1, or an
// `installer_fail=1` cookie, stops it partway), so nothing changes on the server.
// `vite preview` serves the built pages with the same API, for scripts/docs-shots.sh. There,
// ZEABUR_DEMO_INSTALL gives the whole log at the first read: `done`, `running` (stops before
// Zeabur) or `fail`.
import { readFileSync } from "node:fs"
import type { IncomingMessage, ServerResponse } from "node:http"
import { resolve } from "node:path"
import type { Connect, Plugin } from "vite"

const fixtures = resolve(import.meta.dirname, "fixtures")
const fixture = (name: string) => readFileSync(resolve(fixtures, name), "utf8")
const backend = process.env.INSTALLER_GUI
const demo = process.env.ZEABUR_DEMO_INSTALL
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
const log = (fail: boolean) => {
  const lines = fixture("apply.log").trimEnd().split("\n")
  if (!fail) return lines
  const stop = lines.findIndex((l) => l.startsWith("==> Zeabur Identity")) + 2
  return [
    ...lines.slice(0, stop),
    "error: timed out after 5m0s waiting for deployment/zeabur-identity",
  ]
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
    await read(request)
    started = Date.now()
    return json(JSON.stringify({ started: true }))
  }
  if (url.pathname === "/api/apply") {
    const all = log(fail)
    const lines = demo === "running" ? all.slice(0, all.indexOf("==> Zeabur")) : all
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
  if (url.pathname === "/api/state") return json(fixture("state.json"))
  if (url.pathname === "/api/plan") return json(fixture("plan.json"))
  if (url.pathname === "/api/resolve") {
    const name = url.searchParams.get("name") ?? ""
    const address = /\.(\d+\.\d+\.\d+\.\d+)\.sslip\.io$/.exec(name)?.[1]
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
