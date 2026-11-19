// A Secret's `data` values as the objects API sends them, in base64.

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
