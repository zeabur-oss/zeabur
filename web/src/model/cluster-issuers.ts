import type { Schemas } from "@/api/client"

type Issuer = Schemas["IssuerView"]

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
