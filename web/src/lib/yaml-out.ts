// JSON values written as YAML block style, for objects a form made: strings that YAML would read
// as something else are quoted.
type Json = string | number | boolean | null | Json[] | { [key: string]: Json | undefined }

const plain = /^[A-Za-z0-9/._-][A-Za-z0-9/._ :-]*$/
const ambiguous = /^(true|false|yes|no|on|off|y|n|null|~|[-+]?[0-9.][0-9._eE+-]*|0x[0-9a-fA-F]+)$/i

function scalar(value: string | number | boolean | null): string {
  if (typeof value !== "string") return String(value)
  if (value === "" || !plain.test(value) || ambiguous.test(value) || /: |\s$/.test(value))
    return JSON.stringify(value)
  return value
}

const isBlock = (value: Json | undefined): value is Json[] | { [key: string]: Json | undefined } =>
  typeof value === "object" &&
  value !== null &&
  (Array.isArray(value) ? value.length > 0 : Object.keys(value).length > 0)

function lines(value: Json, indent: string): string[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => {
      if (!isBlock(item)) return [`${indent}- ${inline(item)}`]
      const [first = "", ...rest] = lines(item, `${indent}  `)
      return [`${indent}- ${first.trimStart()}`, ...rest]
    })
  }
  if (value !== null && typeof value === "object") {
    return Object.entries(value).flatMap(([key, item]) => {
      if (item === undefined) return []
      const name = scalar(key)
      return isBlock(item)
        ? [`${indent}${name}:`, ...lines(item, Array.isArray(item) ? indent : `${indent}  `)]
        : [`${indent}${name}: ${inline(item)}`]
    })
  }
  return [`${indent}${scalar(value)}`]
}

function inline(value: Json | undefined): string {
  if (Array.isArray(value)) return "[]"
  if (value !== null && typeof value === "object") return "{}"
  return scalar(value ?? null)
}

/** Each object as one YAML document, separated by `---`. */
export function toYaml(objects: readonly unknown[]): string {
  return objects.map((o) => `${lines(o as Json, "").join("\n")}\n`).join("---\n")
}
