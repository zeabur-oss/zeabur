// TanStack Form on Base UI's Field (https://base-ui.com/react/handbook/forms#tanstack-form).
import { useState, type ComponentProps, type ReactNode } from "react"

import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@zeabur-oss/design-system/components/field"
import { Input } from "@zeabur-oss/design-system/components/input"

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

/**
 * An `Input` that shows what was typed and gives its owner the trimmed text. Rewriting the field
 * on every key press moves the cursor and drops a pasted space; an owner that sets another value
 * shows that one.
 */
export function TrimmedInput({
  value,
  onValue,
  ...props
}: Omit<ComponentProps<typeof Input>, "value" | "onChange"> & {
  value: string
  onValue: (value: string) => void
}) {
  const [typed, setTyped] = useState(value)
  return (
    <Input
      {...props}
      value={typed.trim() === value ? typed : value}
      onChange={(event) => {
        setTyped(event.target.value)
        onValue(event.target.value.trim())
      }}
    />
  )
}

/** What a number field's text says: an empty field is NaN, which the owner's checks refuse, not 0. */
export const numberOfText = (text: string) => (text.trim() === "" ? Number.NaN : Number(text))

/** A whole-number field that keeps its text, so a cleared field stays empty instead of showing 0. */
export function NumberInput({
  value,
  onValue,
  ...props
}: Omit<ComponentProps<typeof Input>, "value" | "onChange" | "type" | "inputMode"> & {
  value: number
  onValue: (value: number) => void
}) {
  const [typed, setTyped] = useState(String(value))
  return (
    <Input
      {...props}
      type="text"
      inputMode="numeric"
      value={Object.is(numberOfText(typed), value) ? typed : String(value)}
      onChange={(event) => {
        setTyped(event.target.value)
        onValue(numberOfText(event.target.value))
      }}
    />
  )
}

/** A Kubernetes or S3 name: monospace, trimmed, never autocompleted. */
export function NameField({
  label,
  hint,
  error,
  value,
  onChange,
}: {
  label: ReactNode
  hint?: ReactNode
  /** Shown once there is one. */
  error?: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <FormField label={label} hint={hint} errors={error ? [error] : []}>
      <TrimmedInput
        size="sm"
        mono
        value={value}
        autoComplete="off"
        spellCheck={false}
        onValue={onChange}
      />
    </FormField>
  )
}

/** An instant hint only; the plan checks it again. */
export const quantity = /^\+?(\d+(\.\d*)?|\.\d+)(m|k|Ki|M|Mi|G|Gi|T|Ti|P|Pi|E|Ei|[eE][-+]?\d+)?$/
