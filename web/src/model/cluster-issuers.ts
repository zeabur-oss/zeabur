import type { Schemas } from "@/api/client"

type Issuer = Schemas["IssuerView"]
type Provider = Schemas["Dns01Provider"]["provider"]

/** Each provider's fields: the plain ones in order, then the one secret, which goes in a
 * Secret. Field names are the request's. */
export const PROVIDERS: Record<
  Provider,
  { fields: string[]; optional?: string[]; secret: string }
> = {
  cloudflare: { fields: [], secret: "apiToken" },
  route53: {
    fields: ["region", "accessKeyId", "hostedZoneId"],
    optional: ["hostedZoneId"],
    secret: "secretAccessKey",
  },
  cloudDns: { fields: ["project"], secret: "serviceAccountJson" },
  digitalocean: { fields: [], secret: "token" },
  azureDns: {
    fields: ["subscriptionId", "tenantId", "clientId", "resourceGroupName", "hostedZoneName"],
    secret: "clientSecret",
  },
}

export const PROVIDER_ORDER = Object.keys(PROVIDERS) as Provider[]

export interface IssuerForm {
  name: string
  email: string
  staging: boolean
  provider: Provider
  values: Record<string, string>
}

/** The form for a new issuer, or for one the form can change, its secret blank. */
export function issuerForm(issuer?: Issuer): IssuerForm {
  const { provider = "cloudflare", ...values } = issuer?.dns ?? {}
  return {
    name: issuer?.name ?? "letsencrypt-dns",
    email: issuer?.email ?? "",
    staging: issuer?.staging ?? false,
    provider,
    values: Object.fromEntries(
      Object.entries(values).map(([key, value]) => [key, String(value ?? "")]),
    ),
  }
}

export type IssuerProblem = "name" | "email" | "field" | "secret"

const NAME = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$/

/** What keeps the form from a plan; an issuer being changed keeps its secret when it is blank. */
export function issuerProblems(form: IssuerForm, editing: boolean): IssuerProblem[] {
  const { fields, optional = [], secret } = PROVIDERS[form.provider]
  const value = (key: string) => form.values[key]?.trim() ?? ""
  const problems: IssuerProblem[] = []
  if (form.name.length > 253 || !NAME.test(form.name)) problems.push("name")
  if (form.email.trim() && !form.email.includes("@")) problems.push("email")
  if (fields.some((key) => !optional.includes(key) && !value(key))) problems.push("field")
  if (!editing && !value(secret)) problems.push("secret")
  return problems
}

export function issuerRequest(form: IssuerForm): Schemas["IssuerRequest"] {
  const { fields, secret } = PROVIDERS[form.provider]
  const values = Object.fromEntries(
    [...fields, secret].map((key) => [key, form.values[key]?.trim() ?? ""]),
  )
  return {
    name: form.name,
    email: form.email.trim(),
    staging: form.staging,
    dns: { provider: form.provider, ...values } as Schemas["Dns01Provider"],
  }
}

/** A solver as people write it: `HTTP-01`, or `DNS-01 (<provider>)`. */
export function solverLabel(solver: string): string {
  if (solver.startsWith("dns01:")) return `DNS-01 (${solver.slice(6)})`
  return solver.replace(/^(http|dns)01$/, (_, kind: string) => `${kind.toUpperCase()}-01`)
}

/** Whether `issuer` can sign a certificate for `host`: a wildcard needs DNS-01. */
export const canIssue = (issuer: Pick<Issuer, "wildcards">, host: string) =>
  !host.startsWith("*.") || issuer.wildcards

/** Where a domain's certificate comes from now. */
export type CertificateNow =
  | { as: "issuer"; issuer: string; secret?: string }
  | { as: "secret"; secret: string }
  | { as: "none" }

export function certificateNow(route: Schemas["Route"], host: string): CertificateNow {
  const entry = route.tls?.find((t) => t.host === host)
  if (!entry) return { as: "none" }
  if (route.issuer) return { as: "issuer", issuer: route.issuer, secret: entry.secret ?? undefined }
  return entry.secret ? { as: "secret", secret: entry.secret } : { as: "none" }
}

export type PemProblem = "certificate" | "key"

/** A first look at pasted PEM; the plan checks it fully. */
export function pemProblems(certificate: string, key: string): PemProblem[] {
  const problems: PemProblem[] = []
  if (!certificate.includes("-----BEGIN CERTIFICATE-----")) problems.push("certificate")
  if (!/-----BEGIN (RSA |EC )?PRIVATE KEY-----/.test(key)) problems.push("key")
  return problems
}
