import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import {
  acceptedValue,
  deployProblems,
  filterTemplates,
  generate,
  generateAll,
  inputKind,
  inputProblem,
  inputWords,
  listed,
  startValues,
  templateChanges,
  type AppTemplate,
  type TemplateInput,
} from "@/model/app-templates"

/** The embedded catalog's index.json; the Nix dev shell sets the directory. */
const index = resolve(process.env.ZEABUR_CATALOG_RELEASE ?? "", "index.json")

interface CatalogInput {
  name: string
  generate?: string
  generateBytes?: number
  generateEncoding?: "base64url" | "hex"
  validation?: { pattern?: string }
}

const catalogInputs = (
  JSON.parse(readFileSync(index, "utf8")) as {
    templates: { id: string; inputs?: CatalogInput[] }[]
  }
).templates.flatMap((t) => (t.inputs ?? []).map((input) => ({ template: t.id, input })))

const input = (patch: Partial<TemplateInput> & { name: string }): TemplateInput => ({
  description: "",
  pattern: ".*",
  required: false,
  ...patch,
})

const password = (name: string, secret: string, key: string) =>
  input({
    name,
    pattern: "^[A-Za-z0-9_-]{43}$",
    generate: { bytes: 32, encoding: "base64url" },
    secret: { name: secret, key },
  })

const template = (patch: Partial<AppTemplate>): AppTemplate => ({
  id: "miniflux",
  name: "Miniflux",
  description: "Miniflux feed reader over HTTPS.",
  license: "Apache-2.0",
  homepage: "https://miniflux.app/",
  version: "2.2.0",
  labels: { "app.kubernetes.io/instance": "miniflux" },
  requires: [],
  services: [],
  requests: { cpuMillicores: 150, memoryBytes: 320 * 2 ** 20 },
  inputs: [],
  needsDomain: false,
  needsClusterIssuer: false,
  ...patch,
})

describe("app templates", () => {
  test("a generated value matches its input's pattern in every template", () => {
    const generated = catalogInputs.filter(({ input }) => input.generate === "random")
    expect(generated.length).toBeGreaterThan(0)
    const refused = generated
      .filter(({ input }) => {
        const value = generate({
          bytes: input.generateBytes ?? 32,
          encoding: input.generateEncoding ?? "base64url",
        })
        return !new RegExp(input.validation?.pattern ?? "").test(value)
      })
      .map(({ template, input }) => `${template}.${input.name}`)
    expect(refused).toEqual([])
  })

  test("hex is two digits a byte and base64url has no padding", () => {
    expect(generate({ bytes: 16, encoding: "hex" })).toMatch(/^[0-9a-f]{32}$/)
    expect(generate({ bytes: 32, encoding: "base64url" })).toMatch(/^[A-Za-z0-9_-]{43}$/)
  })

  test("an input that accepts one literal is a consent, with that literal", () => {
    expect(acceptedValue("^I-ACCEPT-DRAGONFLY-BUSL-1\\.1$")).toBe("I-ACCEPT-DRAGONFLY-BUSL-1.1")
    expect(acceptedValue("^[A-Za-z0-9_-]{43}$")).toBeUndefined()
    expect(inputKind(input({ name: "licenseOptIn", pattern: "^I-ACCEPT-MONGODB-SSPL$" }))).toBe(
      "consent",
    )
    expect(inputKind(password("token", "s", "token"))).toBe("generated")
    expect(inputKind(input({ name: "adminEmail" }))).toBe("text")
  })

  test("a value is checked against the whole pattern, and an empty one only when required", () => {
    const cidr = input({
      name: "storageCidr",
      required: true,
      pattern: "([0-9]{1,3}\\.){3}[0-9]{1,3}/([0-9]|[12][0-9]|3[0-2])",
    })
    expect(inputProblem(cidr, "")).toBe("empty")
    expect(inputProblem(cidr, "10.0.0.1")).toBe("invalid")
    expect(inputProblem(cidr, "x10.0.0.1/32")).toBe("invalid")
    expect(inputProblem(cidr, "10.0.0.1/32")).toBeUndefined()
    expect(inputProblem({ ...cidr, required: false }, "")).toBeUndefined()
  })

  test("a missing component, a missing issuer, the domain or an input stops the deploy", () => {
    const email = input({ name: "adminEmail", required: true })
    const ready = template({
      needsDomain: true,
      needsClusterIssuer: true,
      inputs: [email],
    })
    const filled = { adminEmail: "me@example.com" }
    expect(deployProblems(ready, filled, "rss.example.com", "letsencrypt").blocked).toBe(false)
    expect(deployProblems(ready, filled, "rss.example.com", null).blocked).toBe(true)
    expect(deployProblems(ready, filled, "", "letsencrypt")).toMatchObject({
      domain: "empty",
      blocked: true,
    })
    expect(
      deployProblems(ready, { adminEmail: "" }, "rss.example.com", "letsencrypt"),
    ).toMatchObject({ inputs: { adminEmail: "empty" }, blocked: true })
    const absent = template({
      requires: [
        {
          capability: "databases",
          component: "cloudnative-pg",
          name: "CloudNativePG",
          state: "missing",
        },
      ],
    })
    expect(deployProblems(absent, {}, "", null).blocked).toBe(true)
    expect(deployProblems(template({}), {}, "", null).blocked).toBe(false)
  })

  test("an input is shown in the reader's language, else in English", () => {
    const email = input({
      name: "adminEmail",
      label: "Admin email",
      description: "The first account's email.",
      locales: { "zh-TW": { label: "管理員電子郵件", description: "第一個帳號的電子郵件。" } },
    })
    expect(inputWords(email, "zh-TW")).toEqual({
      label: "管理員電子郵件",
      description: "第一個帳號的電子郵件。",
    })
    expect(inputWords(email, "zh-CN")).toEqual({
      label: "Admin email",
      description: "The first account's email.",
    })
    expect(inputWords(input({ name: "token" }), "en").label).toBe("token")
    expect(startValues(template({ inputs: [email] }))).toEqual({ adminEmail: "" })
  })

  test("generated values sharing a Secret go in one create, before the template", () => {
    const wordpress = template({
      id: "wordpress",
      needsDomain: true,
      inputs: [
        password("databasePassword", "wordpress-secrets", "database-password"),
        password("adminPassword", "wordpress-secrets", "admin-password"),
        password("apiKey", "api-secrets", "key"),
        input({ name: "adminEmail", required: true }),
        input({ name: "storageCidr" }),
      ],
    })
    const generated = generateAll(wordpress)
    const changes = templateChanges(
      wordpress,
      { adminEmail: "me@example.com", storageCidr: "" },
      generated,
      "blog.example.com",
    )
    expect<unknown>(changes).toEqual([
      {
        op: "create",
        object: {
          apiVersion: "v1",
          kind: "Secret",
          metadata: { name: "wordpress-secrets", labels: wordpress.labels },
          type: "Opaque",
          stringData: {
            "database-password": generated.databasePassword,
            "admin-password": generated.adminPassword,
          },
        },
      },
      {
        op: "create",
        object: {
          apiVersion: "v1",
          kind: "Secret",
          metadata: { name: "api-secrets", labels: wordpress.labels },
          type: "Opaque",
          stringData: { key: generated.apiKey },
        },
      },
      {
        op: "template",
        template: "wordpress",
        inputs: { adminEmail: "me@example.com" },
        domain: "blog.example.com",
      },
    ])
  })

  test("a template that routes no domain is sent without one", () => {
    const changes = templateChanges(template({ needsDomain: false }), {}, {}, "x.example.com")
    expect<unknown>(changes).toEqual([{ op: "template", template: "miniflux", inputs: {} }])
  })

  test("the list leaves out the page server and filters by name or description", () => {
    const items = [
      template({ id: "umami", name: "Umami", description: "Web analytics" }),
      template({ id: "error-pages", name: "Error pages" }),
      template({ id: "ghost", name: "Ghost", description: "Publishing" }),
    ]
    expect(listed(items).map((t) => t.id)).toEqual(["ghost", "umami"])
    expect(filterTemplates(listed(items), "ANALYTICS").map((t) => t.id)).toEqual(["umami"])
  })
})
