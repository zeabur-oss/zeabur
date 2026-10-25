// Templates of Zeabur Catalog deployed into a project. The server renders a template's objects
// from a `template` change; the browser generates the values the template marks `generate:
// random` and creates their Secrets in the same plan, as the database forms do.
import type { Schemas } from "@/api/client"
import { hostProblem } from "@/model/image-service"

type Change = Schemas["Change"]
export type AppTemplate = Schemas["TemplateView"]
export type TemplateInput = Schemas["TemplateInput"]

/** The page server of the error pages, which a domain's settings deploy. */
const ELSEWHERE = new Set(["error-pages"])

/** The templates "App template" lists, by name. */
export const listed = (items: readonly AppTemplate[]) =>
  items.filter((t) => !ELSEWHERE.has(t.id)).sort((a, b) => a.name.localeCompare(b.name))

/** Templates whose name, id or description holds `query`, ignoring case. */
export function filterTemplates(items: readonly AppTemplate[], query: string) {
  const q = query.trim().toLowerCase()
  if (!q) return [...items]
  return items.filter((t) => [t.name, t.id, t.description].some((s) => s.toLowerCase().includes(q)))
}

/** The components the template needs that the cluster does not serve. */
export const missing = (template: AppTemplate) =>
  template.requires.filter((r) => r.state === "missing")

/** The value an input accepts when its pattern accepts one literal only (`^I-ACCEPT-X$`). */
export function acceptedValue(pattern: string): string | undefined {
  const literal = /^\^((?:[A-Za-z0-9-]|\\\.)+)\$$/.exec(pattern)?.[1]
  return literal?.replaceAll("\\.", ".")
}

export type InputKind = "generated" | "consent" | "text"

export function inputKind(input: TemplateInput): InputKind {
  if (input.generate) return "generated"
  return acceptedValue(input.pattern) === undefined ? "text" : "consent"
}

/** The inputs a person sees, in the catalogue's order. */
export const shownInputs = (template: AppTemplate) =>
  template.inputs.filter((i) => inputKind(i) !== "generated")

/** Values to start the form with: the kube-apiserver address the cluster gave, when the template
 * asks for it. */
export function startValues(template: AppTemplate, kubeApiCidr?: string | null) {
  const values: Record<string, string> = {}
  for (const input of shownInputs(template)) values[input.name] = ""
  if (kubeApiCidr && "kubeApiCidr" in values) values.kubeApiCidr = kubeApiCidr
  return values
}

/** Random bytes, encoded as the template asks. */
export function generate({ bytes, encoding }: Schemas["Generated"]): string {
  const random = crypto.getRandomValues(new Uint8Array(bytes))
  if (encoding === "hex") return Array.from(random, (b) => b.toString(16).padStart(2, "0")).join("")
  return btoa(String.fromCharCode(...random))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "")
}

/** A value for each generated input. */
export function generateAll(template: AppTemplate): Record<string, string> {
  const values: Record<string, string> = {}
  for (const input of template.inputs)
    if (input.generate) values[input.name] = generate(input.generate)
  return values
}

/** Why a value is refused: a required one left empty, or one the pattern does not match. A
 * pattern JavaScript cannot read is left to the server. */
export function inputProblem(input: TemplateInput, value: string): "empty" | "invalid" | undefined {
  if (value === "") return input.required ? "empty" : undefined
  let pattern: RegExp
  try {
    pattern = new RegExp(`^(?:${input.pattern})$`)
  } catch {
    return undefined
  }
  return pattern.test(value) ? undefined : "invalid"
}

/** What stops the deploy: a component the cluster lacks, no issuer when the template needs one,
 * the domain, or an input. */
export function deployProblems(
  template: AppTemplate,
  values: Record<string, string>,
  host: string,
  issuer?: string | null,
) {
  const domain = template.needsDomain ? hostProblem(host) : undefined
  const inputs = Object.fromEntries(
    shownInputs(template).map((i) => [i.name, inputProblem(i, values[i.name] ?? "")]),
  )
  const requirements = missing(template).length > 0 || (template.needsClusterIssuer && !issuer)
  const blocked =
    requirements || domain !== undefined || Object.values(inputs).some((p) => p !== undefined)
  return { domain, inputs, blocked }
}

/** The names of the Secrets the generated values go in. */
export const generatedSecrets = (template: AppTemplate) => [
  ...new Set(template.inputs.flatMap((i) => (i.generate && i.secret ? [i.secret.name] : []))),
]

/** One Secret for each name the generated inputs share, then the template. Empty optional
 * values are left out, as the catalogue renders a missing one as empty. */
export function templateChanges(
  template: AppTemplate,
  values: Record<string, string>,
  generated: Record<string, string>,
  domain?: string,
): Change[] {
  const secrets = new Map<string, Record<string, string>>()
  for (const input of template.inputs) {
    const value = generated[input.name]
    if (!input.generate || !input.secret || value === undefined) continue
    const data = secrets.get(input.secret.name) ?? {}
    data[input.secret.key] = value
    secrets.set(input.secret.name, data)
  }
  const inputs = Object.fromEntries(
    shownInputs(template).flatMap((i) => (values[i.name] ? [[i.name, values[i.name]]] : [])),
  )
  return [
    ...[...secrets].map(([name, stringData]): Change => {
      const secret: Record<string, unknown> = {
        apiVersion: "v1",
        kind: "Secret",
        metadata: { name },
        type: "Opaque",
        stringData,
      }
      return { op: "create", object: secret as Record<string, never> }
    }),
    {
      op: "template",
      template: template.id,
      inputs,
      ...(template.needsDomain && domain ? { domain } : {}),
    },
  ]
}
