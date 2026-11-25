// A form's check of a text field by the rule the server's OpenAPI description sets on it.

interface TextRule {
  readonly maxLength?: number
  readonly pattern?: string
}

/** Why `value` breaks `rule`: longer than it allows, or not of its pattern. */
export function textProblem(value: string, rule: TextRule): "long" | "invalid" | undefined {
  if (rule.maxLength !== undefined && [...value].length > rule.maxLength) return "long"
  if (rule.pattern !== undefined && !new RegExp(rule.pattern).test(value)) return "invalid"
  return undefined
}

export const fits = (value: string, rule: TextRule): boolean =>
  textProblem(value, rule) === undefined

interface NumberRule {
  readonly minimum?: number
  readonly maximum?: number
}

/** Whether `value` is a whole number the server's `rule` takes. */
export function isWholeNumber(value: string, rule: NumberRule): boolean {
  if (!/^\d+$/.test(value)) return false
  const n = Number(value)
  return n >= (rule.minimum ?? 0) && n <= (rule.maximum ?? Infinity)
}
