// One field of a form, as every form here writes it: a TanStack Form field shown as a Base UI
// Field (https://base-ui.com/react/handbook/forms#tanstack-form), with its label, its control, a
// hint, and the first of its errors (the form's own check, or the cluster's refusal of the field
// in the last plan). The checks in the forms are instant hints; the server's plan decides.
import type { ReactNode } from "react"

import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"

/** The first message among a field's errors (TanStack Form's, and `refusedField`'s). */
const firstMessage = (errors: unknown[]) =>
  errors.find((e): e is string => typeof e === "string" && e !== "")

export function FormField({
  label,
  hint,
  errors = [],
  hideLabel,
  disabled,
  className,
  children,
}: {
  label: ReactNode
  hint?: ReactNode
  errors?: unknown[]
  /** The label is for assistive technology only: a row whose columns say what each field is. */
  hideLabel?: boolean
  disabled?: boolean
  className?: string
  children: ReactNode
}) {
  const message = firstMessage(errors)
  return (
    <Field invalid={message !== undefined} disabled={disabled} className={className}>
      <FieldLabel className={hideLabel ? "sr-only" : undefined}>{label}</FieldLabel>
      {children}
      {hint ? <FieldDescription>{hint}</FieldDescription> : null}
      <FieldError match={message !== undefined}>{message}</FieldError>
    </Field>
  )
}

/** A Kubernetes quantity as the API server accepts it (`500m`, `1.5`, `256Mi`, `2G`), for an
 * instant hint while typing; the plan checks it again. */
export const quantity = /^\+?(\d+(\.\d*)?|\.\d+)(m|k|Ki|M|Mi|G|Gi|T|Ti|P|Pi|E|Ei|[eE][-+]?\d+)?$/
