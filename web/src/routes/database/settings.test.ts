import { describe, expect, test } from "bun:test"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type { Schemas } from "@/api/client"
import { databaseUpdaterQuery, namespaceQuery, whoamiQuery } from "@/api/queries"
import type { Database } from "@/model/databases"
import { DatabaseSettings } from "@/routes/database/settings"

const database: Database = {
  apiVersion: "postgresql.cnpg.io/v1",
  apiGroup: "postgresql.cnpg.io",
  apiGroupVersion: "v1",
  kind: "Cluster",
  resource: "clusters",
  engine: "postgresql",
  provider: "cloudnative-pg",
  name: "orders",
  state: "ready",
  members: 1,
  ready: 1,
  primary: "orders-1",
  claims: [],
  pods: [],
  storage: "1Gi",
  image: "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie",
  origin: {
    apiVersion: "postgresql.cnpg.io/v1",
    kind: "Cluster",
    name: "orders",
    namespace: "shop",
  },
  connection: {
    host: "orders-rw.shop.svc",
    port: 5432,
    secret: "orders-app",
    passwordKey: "password",
  },
  backups: { schedules: [], runs: [], ready: false },
  object: {
    apiVersion: "postgresql.cnpg.io/v1",
    kind: "Cluster",
    metadata: { name: "orders", namespace: "shop" },
    spec: { imageName: "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie", instances: 1 },
  } as unknown as Database["object"],
}

const updater = (status: Schemas.DatabaseUpdaterStatus): Schemas.DatabaseUpdater => ({
  status,
  message: "Server explanation",
})

function client(state?: Schemas.DatabaseUpdater, readOnly = false) {
  const query = new QueryClient()
  query.setQueryData(whoamiQuery.queryKey, {
    member: true,
    username: "developer",
    groups: [],
    admin: false,
  })
  query.setQueryData(namespaceQuery("shop").queryKey, {
    name: "shop",
    readOnly,
    adopted: true,
    canAdopt: "denied",
    canChange: readOnly ? "denied" : "allowed",
    canReadLogs: readOnly ? "denied" : "allowed",
    canReadSecrets: readOnly ? "denied" : "allowed",
    labelsGuarded: true,
    system: false,
    origin: { apiVersion: "v1", kind: "Namespace", name: "shop" },
  })
  if (state) query.setQueryData(databaseUpdaterQuery.queryKey, state)
  return query
}

function settings(query: QueryClient, db = database) {
  const html = renderToStaticMarkup(
    createElement(
      QueryClientProvider,
      { client: query },
      createElement(DatabaseSettings, { project: "shop", database: db, onMode: () => {} }),
    ),
  )
  return html.slice(html.indexOf('<section id="updates"'))
}

function policyDisabled(html: string) {
  const button = html.match(/<button[^>]*role="combobox"[^>]*>/)?.[0]
  expect(button).toBeDefined()
  return /\sdisabled(?:=|\s|>)/.test(button!)
}

describe("Database updater readiness", () => {
  test("enabling updates needs an observed ready updater", () => {
    const ready = settings(client(updater("ready")))
    expect(ready).toContain("Automatic database updates are ready.")
    expect(policyDisabled(ready)).toBe(false)
    for (const status of ["missing", "notReady", "unknown"] as const) {
      expect(policyDisabled(settings(client(updater(status))))).toBe(true)
    }
    const pending = settings(client())
    expect(pending).toContain("Checking automatic database updates")
    expect(policyDisabled(pending)).toBe(true)
  })

  test("a Viewer can see readiness without acquiring update permission", () => {
    const html = settings(client(updater("ready"), true))
    expect(html).toContain("Automatic database updates are ready.")
    expect(policyDisabled(html)).toBe(true)
  })

  test("permission refusals and unavailable status give different explanations", () => {
    const denied = {
      ...updater("unknown"),
      problem: { kind: "forbidden" } as Schemas.Problem,
    }
    const refused = settings(client(denied))
    expect(refused).toContain("Your account can&#x27;t check the database updater")
    expect(policyDisabled(refused)).toBe(true)
    const unavailable = settings(client(updater("unknown")))
    expect(unavailable).toContain("The database updater couldn&#x27;t be checked")
    expect(policyDisabled(unavailable)).toBe(true)
  })

  test("a failed refresh cannot leave enabling allowed by a cached ready answer", async () => {
    const query = client(updater("ready"))
    await expect(
      query.query({
        ...databaseUpdaterQuery,
        staleTime: 0,
        queryFn: () => Promise.reject(new Error("connection lost")),
      }),
    ).rejects.toThrow("connection lost")
    const html = settings(query)
    expect(html).toContain("The database updater couldn&#x27;t be checked")
    expect(policyDisabled(html)).toBe(true)
  })

  test("existing unsupported settings remain editable to Off without a ready updater", () => {
    const existing = {
      ...database,
      object: {
        ...database.object,
        metadata: {
          name: database.name,
          annotations: { "keel.sh/policy": "glob:legacy-*" },
        },
        spec: { imageCatalogRef: { name: "catalog", major: 17 } },
      } as unknown as Database["object"],
    }
    for (const status of ["missing", "notReady", "unknown"] as const) {
      const html = settings(client(updater(status)), existing)
      expect(html).toContain("Set outside Zeabur: glob:legacy-*")
      expect(policyDisabled(html)).toBe(false)
    }
    expect(policyDisabled(settings(client(updater("missing"), true), existing))).toBe(true)
  })
})
