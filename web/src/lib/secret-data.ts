// A Secret's `data` read from the YAML the objects API writes: top-level `data:`, one key per
// line indented by two spaces, scalars bare or quoted as kubectl quotes them.

function unquote(scalar: string): string {
  if (scalar.length >= 2 && scalar.startsWith('"') && scalar.endsWith('"'))
    return JSON.parse(scalar) as string
  if (scalar.length >= 2 && scalar.startsWith("'") && scalar.endsWith("'"))
    return scalar.slice(1, -1).replaceAll("''", "'")
  return scalar
}

const entry = /^ {2}("(?:[^"\\]|\\.)*"|'(?:[^']|'')*'|[^\s"'][^:]*):(?: (.*))?$/

/** Each key of the Secret's `data` and its value in base64. */
export function secretData(yaml: string): Record<string, string> {
  const data: Record<string, string> = Object.create(null)
  let inData = false
  for (const line of yaml.split("\n")) {
    if (/^data:\s*$/.test(line)) inData = true
    else if (/^\S/.test(line)) inData = false
    else if (inData) {
      const match = line.match(entry)
      if (match?.[1]) data[unquote(match[1])] = unquote((match[2] ?? "").trim())
    }
  }
  return data
}

export interface SecretValue {
  bytes: Uint8Array<ArrayBuffer>
  /** The bytes as UTF-8 text; `undefined` when they are not valid UTF-8. */
  text: string | undefined
}

/** A `data` value decoded; `undefined` when it is not base64. */
export function decodeSecretValue(base64: string): SecretValue | undefined {
  let binary: string
  try {
    binary = atob(base64)
  } catch {
    return undefined
  }
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
  try {
    return { bytes, text: new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes) }
  } catch {
    return { bytes, text: undefined }
  }
}
