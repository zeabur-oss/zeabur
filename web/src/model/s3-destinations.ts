// The cluster's S3 destinations: a picker for backup forms. A plan copies a destination's keys
// into a Secret of the project, so no backup reads the list.
import type { Schemas } from "@/api/client"

type Change = Schemas["Change"]
export type Destination = Schemas["S3Destination"]

/** The fields a destination fills in a form. */
export interface S3Target {
  endpoint: string
  bucket: string
  path: string
  region: string
  accessKey: string
  secretKey: string
  /** The destination the keys are copied from; typed keys when absent. */
  destination?: string
}

/** `folder` under the destination's own folder. */
export const joinFolder = (prefix: string, folder = "") =>
  [prefix, folder]
    .map((part) => part.replace(/^\/+|\/+$/g, ""))
    .filter(Boolean)
    .join("/")

/** The fields `destination` fills; its keys stay on the cluster. `folder` is the form's own
 * default folder. */
export const fromDestination = (destination: Destination, folder?: string): Partial<S3Target> => ({
  destination: destination.name,
  endpoint: destination.endpoint,
  bucket: destination.bucket,
  region: destination.region,
  path: joinFolder(destination.prefix, folder),
  accessKey: "",
  secretKey: "",
})

/** Back to typing the keys, with the fields as they are. */
export const manual = (): Partial<S3Target> => ({ destination: undefined })

/** The keys a form needs: none when they are copied from a destination. */
export const needsKeys = (target: S3Target) =>
  target.destination === undefined &&
  (target.accessKey.trim() === "" || target.secretKey.trim() === "")

/** The Secret `name` with the target's keys under `keys` and the other values in `more`: typed,
 * or copied from its destination when the plan is made. */
export function keysSecret(
  name: string,
  target: S3Target,
  keys: { accessKeyId: string; secretAccessKey: string },
  more: Record<string, string> = {},
): Change {
  const object = { apiVersion: "v1", kind: "Secret", metadata: { name }, type: "Opaque" }
  if (target.destination !== undefined)
    return {
      op: "copyS3Keys",
      destination: target.destination,
      object: { ...object, stringData: more } as unknown as Record<string, never>,
      ...keys,
    }
  return {
    op: "create",
    object: {
      ...object,
      stringData: {
        [keys.accessKeyId]: target.accessKey.trim(),
        [keys.secretAccessKey]: target.secretKey.trim(),
        ...more,
      },
    } as unknown as Record<string, never>,
  }
}

export const AWS_KEYS = {
  accessKeyId: "AWS_ACCESS_KEY_ID",
  secretAccessKey: "AWS_SECRET_ACCESS_KEY",
}

export interface DestinationForm {
  name: string
  endpoint: string
  bucket: string
  region: string
  prefix: string
  accessKeyId: string
  secretAccessKey: string
}

export const destinationForm = (destination?: Destination): DestinationForm => ({
  name: destination?.name ?? "",
  endpoint: destination?.endpoint ?? "",
  bucket: destination?.bucket ?? "",
  region: destination?.region ?? "",
  prefix: destination?.prefix ?? "",
  accessKeyId: "",
  secretAccessKey: "",
})

export type DestinationProblem = "name" | "taken" | "endpoint" | "bucket" | "prefix" | "keys"

/** Keys are needed for a new destination; when editing, both or neither. */
export function destinationProblems(
  form: DestinationForm,
  taken: string[],
  editing: boolean,
): DestinationProblem[] {
  const problems: DestinationProblem[] = []
  if (!/^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/.test(form.name)) problems.push("name")
  else if (!editing && taken.includes(form.name)) problems.push("taken")
  if (!/^https?:\/\/[^\s/]+\S*$/.test(form.endpoint.trim())) problems.push("endpoint")
  if (!/^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/.test(form.bucket.trim())) problems.push("bucket")
  if (form.prefix.includes("..") || /\s/.test(form.prefix)) problems.push("prefix")
  const typed = [form.accessKeyId.trim(), form.secretAccessKey.trim()].filter(Boolean).length
  if (typed === 1 || (!editing && typed === 0)) problems.push("keys")
  return problems
}

export const saveRequest = (
  form: DestinationForm,
): Extract<Schemas["S3DestinationsPlanRequest"], { op: "save" }> => ({
  op: "save",
  name: form.name,
  endpoint: form.endpoint.trim(),
  bucket: form.bucket.trim(),
  region: form.region.trim(),
  prefix: form.prefix.trim(),
  accessKeyId: form.accessKeyId.trim(),
  secretAccessKey: form.secretAccessKey.trim(),
})
