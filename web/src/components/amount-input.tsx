// An input for CPU, memory or storage that shows plain units ("0.1 cores", "128 MiB") and still
// takes Kubernetes' ("100m", "128Mi"). The value outside is always Kubernetes' form, or the text
// as typed when it is no amount.
import { useEffect, useState, type ComponentProps } from "react"

import { Input } from "@zeabur-oss/design-system/components/input"
import { parseAmount, toKube, type Dimension } from "@/lib/amount"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"

type InputProps = Omit<ComponentProps<typeof Input>, "value" | "onChange" | "size">

export function AmountInput({
  value,
  onChange,
  dimension,
  ...props
}: InputProps & {
  /** Kubernetes' form: `100m`, `5Gi`. */
  value: string
  onChange: (value: string) => void
  dimension: Dimension
}) {
  const { exact } = useQuantityFormat()
  const [text, setText] = useState(() => exact(value, dimension))
  useEffect(() => {
    setText((typed) =>
      toKube(typed, dimension) === value.trim() ? typed : exact(value, dimension),
    )
  }, [value, dimension, exact])
  return (
    <Input
      size="sm"
      mono
      autoComplete="off"
      spellCheck={false}
      {...props}
      value={text}
      onChange={(event) => {
        const typed = event.target.value
        setText(typed)
        const next = toKube(typed, dimension)
        const same =
          parseAmount(next, dimension) !== undefined &&
          parseAmount(next, dimension) === parseAmount(value, dimension)
        if (!same) onChange(next)
      }}
    />
  )
}

/** An amount in plain units as text; `fallback` where there is none. */
export function Amount({
  value,
  dimension,
  fallback = "",
}: {
  value: string | null | undefined
  dimension: Dimension
  fallback?: string
}) {
  const { amount } = useQuantityFormat()
  return value ? amount(value, dimension) : fallback
}

/** The sentence under a size choice: what each instance reserves and may use. */
export function SizeDetail({
  resources,
}: {
  resources: { requests: { cpu: string; memory: string }; limits: { memory: string } }
}) {
  const t = useT()
  const { amount } = useQuantityFormat()
  return t(
    "db.sizeDetail",
    amount(resources.requests.cpu, "cpu"),
    amount(resources.requests.memory, "bytes"),
    amount(resources.limits.memory, "bytes"),
  )
}
