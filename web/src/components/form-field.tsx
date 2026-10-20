// TanStack Form on Base UI's Field (https://base-ui.com/react/handbook/forms#tanstack-form).
import type { ReactNode } from "react"

import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@zeabur/design-system/components/field"

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

/** An instant hint only; the plan checks it again. */
export const quantity = /^\+?(\d+(\.\d*)?|\.\d+)(m|k|Ki|M|Mi|G|Gi|T|Ti|P|Pi|E|Ei|[eE][-+]?\d+)?$/
