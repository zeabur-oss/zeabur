import type { CatalogComponent, Input } from "@/api"

/** How the page asks for an input. */
export type InputKind = "text" | "secret" | "multiline" | "url" | "boolean"

/** Inputs whose kind the catalog does not tell, by `<component>:<input>`. */
const kinds: Record<string, InputKind> = {
  "volume-backup/velero:s3Url": "url",
  "volume-backup/velero:credentials": "multiline",
  "volume-backup/velero:fileBackup": "boolean",
  // An ID, not a secret, though the catalog keeps it in a Secret.
  "remote-access/tailscale-operator:oauthClientId": "text",
}

export function inputKind(component: string, input: Input): InputKind {
  const kind = kinds[`${component}:${input.name}`]
  if (kind) return kind
  if (input.secret) return "secret"
  return typeof input.default === "boolean" ? "boolean" : "text"
}

export const velero = "volume-backup/velero"

/** The catalog inputs the Velero form asks for in its own fields. */
export const veleroInputs = ["s3Url", "bucket", "credentials"]

/** The secrets the page sends in place of `credentials`; gui.rs makes the file from them. */
export const veleroKeys = ["access-key-id", "secret-access-key"] as const

/** Whether the page shows the Velero form: the catalog still has the inputs it fills. */
export function veleroForm(component: CatalogComponent): boolean {
  return (
    component.id === velero &&
    veleroInputs.every((name) => component.inputs.some((input) => input.name === name))
  )
}

interface Given {
  inputs: Record<string, Record<string, unknown>>
  secrets: Record<string, Record<string, string>>
}

const filled = (value: unknown) => value !== undefined && value !== null && value !== ""

/** Whether a chosen component still lacks a required value. */
export function lacksInput(component: CatalogComponent, given: Given): boolean {
  const inputs = given.inputs[component.id] ?? {}
  const secrets = given.secrets[component.id] ?? {}
  const form = veleroForm(component)
  if (form && !veleroKeys.every((key) => filled(secrets[key]?.trim()))) return true
  return component.inputs.some(
    (input) =>
      input.required &&
      !input.filled &&
      !filled(input.default) &&
      !(form && input.name === "credentials") &&
      !filled(input.secret ? secrets[input.name] : inputs[input.name]),
  )
}
