// What the desktop shots (desktop.ts) show: the answers of the app's Tauri commands, which
// `install` mocks in the page with `mockIPC`. The shapes are the types of
// desktop/web/src/chooser/api.ts. desktop.ts bundles this file for the browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks"
import { emit } from "@tauri-apps/api/event"

const kube = "/Users/demo/.kube/config"
const team = "/Users/demo/clusters/team"

const sources = [
  { id: "default", kind: "default", path: kube, files: 1, contexts: 3, errors: [], missing: false },
  {
    id: "team",
    kind: "folder",
    path: team,
    files: 4,
    contexts: 3,
    errors: [{ file: `${team}/old.yaml`, reason: "did not find expected key at line 4" }],
    missing: false,
  },
]

type Entry = {
  name: string
  file: string
  cluster: string
  server: string | null
  user: string | null
  namespace?: string
  current?: boolean
  sameNameElsewhere?: number
  auth: Record<string, unknown>
}

const entries: Entry[] = [
  {
    name: "zeabur-demo",
    file: kube,
    cluster: "zeabur-demo",
    server: "http://127.0.0.1:8080",
    user: "demo",
    current: true,
    auth: { state: "ok", method: "token" },
  },
  {
    name: "staging",
    file: kube,
    cluster: "staging",
    server: "https://staging.k8s.example.com:6443",
    user: "staging-admin",
    namespace: "shop",
    sameNameElsewhere: 1,
    auth: { state: "ok", method: "token" },
  },
  {
    name: "office-k3s",
    file: kube,
    cluster: "office-k3s",
    server: "https://192.0.2.10:6443",
    user: "office-admin",
    auth: { state: "fileMissing", path: "/Users/demo/.kube/certs/office-admin.crt" },
  },
  {
    name: "staging",
    file: `${team}/aks.yaml`,
    cluster: "aks-staging",
    server: "https://aks-staging-dns-1a2b3c.hcp.japaneast.azmk8s.io:443",
    user: "aks-staging-user",
    sameNameElsewhere: 1,
    auth: { state: "pluginMissing", command: "kubelogin", hint: "Install kubelogin." },
  },
  {
    name: "prod-tokyo",
    file: `${team}/eks.yaml`,
    cluster: "arn:aws:eks:ap-northeast-1:111122223333:cluster/prod-tokyo",
    server: "https://0123456789ABCDEF0123456789ABCDEF.gr7.ap-northeast-1.eks.amazonaws.com",
    user: "arn:aws:eks:ap-northeast-1:111122223333:cluster/prod-tokyo",
    auth: { state: "ok", method: "exec" },
  },
  {
    name: "lab",
    file: `${team}/split.yaml`,
    cluster: "lab",
    server: null,
    user: "lab-admin",
    auth: { state: "missingEntry", what: "cluster", name: "lab" },
  },
]

const contexts = entries.map((entry) => {
  const sourceId = entry.file === kube ? "default" : "team"
  return {
    key: `${entry.file}#${entry.name}`,
    sourceId,
    current: false,
    sameNameElsewhere: 0,
    namespace: null,
    ...entry,
  }
})

const key = (name: string, file = kube) => `${file}#${name}`

const snapshot = {
  sources,
  contexts,
  favourites: [key("prod-tokyo", `${team}/eks.yaml`)],
  recents: [key("zeabur-demo")],
  shell: { state: "done" },
}

const reaches: Record<string, unknown> = {
  [key("zeabur-demo")]: { state: "reachable", millis: 4 },
  [key("staging")]: { state: "reachable", millis: 38 },
  [key("office-k3s")]: { state: "timedOut" },
  [key("staging", `${team}/aks.yaml`)]: { state: "reachable", millis: 91 },
  [key("prod-tokyo", `${team}/eks.yaml`)]: { state: "reachable", millis: 52 },
  [key("lab", `${team}/split.yaml`)]: { state: "noAddress" },
}

/** The kubeconfig the paste dialog shows, the same as desktop/fixtures/paste-sample.yaml. */
export const pasteSample = `apiVersion: v1
kind: Config
clusters:
  - name: homelab
    cluster:
      server: https://homelab.local:6443
users:
  - name: homelab-admin
    user:
      token: not-a-real-token
contexts:
  - name: homelab
    context:
      cluster: homelab
      user: homelab-admin
`

export type Scenario = {
  /** False is a first run, with no kubeconfig found. */
  sources: boolean
  /** The reason the cluster page shows when the service does not start. */
  failure?: string
}

export function install(scenario: Scenario) {
  mockWindows("main")
  mockIPC(
    (command, args) => {
      switch (command) {
        case "snapshot":
          return scenario.sources
            ? snapshot
            : { sources: [], contexts: [], favourites: [], recents: [], shell: { state: "done" } }
        case "probe": {
          const [[id, server]] = (args as { targets: [string, string | null][] }).targets
          return [{ key: id, reach: server === null ? { state: "noAddress" } : reaches[id] }]
        }
        case "start_cluster":
          return Promise.reject(scenario.failure ?? "")
        default:
          return null
      }
    },
    { shouldMockEvents: true },
  )
}

// What desktop.ts calls in the page once it has bundled this file.
export const demo = { install, emit }
Object.assign(globalThis, { demo })
