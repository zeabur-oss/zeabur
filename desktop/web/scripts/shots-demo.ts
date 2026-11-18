// What the desktop screenshots show: the answers of the app's Tauri commands, which `install`
// mocks in the page with `mockIPC`. web/e2e/shots/desktop.ts bundles this file for the browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks"

import type { Context, Snapshot } from "../src/chooser/api.ts"

const kube = "/Users/demo/.kube/config"
const team = "/Users/demo/clusters"

type Entry = Omit<Context, "key" | "sourceId" | "current" | "namespace"> &
  Partial<Pick<Context, "current" | "namespace">>

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

const sources: Snapshot["sources"] = [
  { id: "default", kind: "default", path: kube, contexts: 3, error: null, missing: false },
  ...["aks", "eks", "split"].map((name) => ({
    id: name,
    kind: "file" as const,
    path: `${team}/${name}.yaml`,
    contexts: 1,
    error: null,
    missing: false,
  })),
  {
    id: "old",
    kind: "file",
    path: `${team}/old.yaml`,
    contexts: 0,
    error: "did not find expected key at line 4",
    missing: false,
  },
]

const contexts: Context[] = entries.map((entry) => ({
  key: `${entry.file}#${entry.name}`,
  sourceId: entry.file === kube ? "default" : (entry.file.split("/").pop()?.split(".")[0] ?? ""),
  current: false,
  namespace: null,
  ...entry,
}))

export type Scenario = {
  /** False is a first run, with no kubeconfig found. */
  sources: boolean
  /** The reason the cluster page shows when the service does not start. */
  failure?: string
}

export function install(scenario: Scenario) {
  const snapshot: Snapshot = scenario.sources
    ? { sources, contexts, shell: { state: "done" } }
    : { sources: [], contexts: [], shell: { state: "done" } }
  mockWindows("main")
  mockIPC(
    (command) => {
      switch (command) {
        case "snapshot":
          return snapshot
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
Object.assign(globalThis, { demo: { install } })
