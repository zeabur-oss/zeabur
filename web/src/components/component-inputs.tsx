// The values a catalog component asks for before Zeabur installs it.
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { useT, type Translate } from "@/i18n"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Label } from "@zeabur-oss/design-system/components/label"
import { Switch } from "@zeabur-oss/design-system/components/switch"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

export type Values = Schemas.ComponentRequestValues

const given = (values: Values, input: Schemas.ComponentInput) =>
  values[input.component]?.[input.name]

const withValue = (values: Values, input: Schemas.ComponentInput, value: Schemas.InputValue) => ({
  ...values,
  [input.component]: { ...values[input.component], [input.name]: value },
})

/** What a field shows: what the person typed, else the value the plan keeps. A secret is never
 * shown again. */
const shownOf = (values: Values, input: Schemas.ComponentInput) =>
  given(values, input) ??
  (input.kind !== "secret" && (input.state === "given" || input.state === "kept")
    ? input.value
    : undefined)

const keyOf = (input: Schemas.ComponentInput) => `${input.component}/${input.name}`

export const isMissing = (inputs: Schemas.ComponentInput[]) =>
  inputs.some((i) => i.state === "missing")

/** A field for each input, grouped by component when more than one asks for values. */
export function InputsForm({
  inputs,
  values,
  checked,
  onChange,
}: {
  inputs: Schemas.ComponentInput[]
  values: Values
  checked: boolean
  onChange: (values: Values) => void
}) {
  const t = useT()
  const components = [...new Set(inputs.map((i) => i.component))]
  return (
    <section className="flex flex-col gap-4">
      <h3 className="type-meta font-medium text-muted-foreground">{t("components.values")}</h3>
      {components.map((component) => (
        <div key={component} className="flex flex-col gap-4">
          {components.length > 1 ? (
            <h4 className="font-medium" translate="no">
              {component}
            </h4>
          ) : null}
          {inputs
            .filter((i) => i.component === component)
            .map((input) => (
              <InputField
                key={keyOf(input)}
                input={input}
                value={shownOf(values, input)}
                invalid={
                  checked && input.state === "missing" && !shownOf(values, input)?.toString()
                }
                onValue={(value) => onChange(withValue(values, input, value))}
              />
            ))}
        </div>
      ))}
    </section>
  )
}

function InputField({
  input,
  value,
  invalid,
  onValue,
}: {
  input: Schemas.ComponentInput
  value: Schemas.InputValue | null | undefined
  invalid: boolean
  onValue: (value: Schemas.InputValue) => void
}) {
  const t = useT()
  const id = `input-${input.component}-${input.name}`
  if (input.kind === "flag")
    return <FlagField id={id} input={input} on={value === true} onValue={onValue} />
  const label = (
    <>
      <span className="font-mono" translate="no">
        {input.name}
      </span>
      {input.required ? (
        <span className="ml-2 type-meta text-muted-foreground">{t("components.required")}</span>
      ) : null}
    </>
  )
  return (
    <FormField
      label={label}
      hint={hintOf(t, input)}
      errors={invalid ? [t("components.needsValue")] : []}
    >
      <TextField input={input} text={textOf(value)} onValue={onValue} />
    </FormField>
  )
}

function FlagField({
  id,
  input,
  on,
  onValue,
}: {
  id: string
  input: Schemas.ComponentInput
  on: boolean
  onValue: (value: boolean) => void
}) {
  return (
    <div className="flex items-start gap-2.5">
      <Switch id={id} size="sm" checked={on} onCheckedChange={onValue} />
      <div className="flex flex-col gap-0.5">
        <Label htmlFor={id}>
          <span className="font-mono" translate="no">
            {input.name}
          </span>
        </Label>
        <p className="type-meta text-pretty text-muted-foreground">{input.description}</p>
      </div>
    </div>
  )
}

function TextField({
  input,
  text,
  onValue,
}: {
  input: Schemas.ComponentInput
  text: string
  onValue: (value: Schemas.InputValue) => void
}) {
  if (input.kind === "secret")
    return (
      <Textarea
        rows={3}
        value={text}
        autoComplete="off"
        spellCheck={false}
        translate="no"
        onChange={(event) => onValue(event.target.value)}
      />
    )
  const integer = input.kind === "integer"
  return (
    <Input
      type={integer ? "number" : "text"}
      inputMode={integer ? "numeric" : undefined}
      value={text}
      placeholder={textOf(input.default ?? pickedOf(input)) || undefined}
      autoComplete="off"
      spellCheck={false}
      translate="no"
      onChange={(event) => onValue(integer ? integerOf(event.target.value) : event.target.value)}
    />
  )
}

/** The value Zeabur picked when the field is left empty; never a secret's. */
const pickedOf = (input: Schemas.ComponentInput) =>
  input.state === "generated" && input.kind !== "secret" ? input.value : undefined

const textOf = (value: Schemas.InputValue | null | undefined) =>
  value === undefined || value === null ? "" : String(value)

/** A whole number as a number; anything else as typed, for the server to refuse. */
function integerOf(typed: string): Schemas.InputValue {
  const number = Number(typed)
  return typed.trim() !== "" && Number.isInteger(number) ? number : typed
}

/** The catalog's description, then where a secret's value comes from when the field is empty. */
function hintOf(t: Translate, input: Schemas.ComponentInput) {
  const kept = input.kind === "secret" && input.state === "kept"
  const generated = input.generated && input.state !== "kept"
  return [
    input.description,
    kept ? t("components.keptSecret") : "",
    generated ? t("components.generatedSecret") : "",
  ]
    .filter(Boolean)
    .join(" ")
}

/** What the plan writes for each input; a secret only says where its value comes from. */
export function InputsReview({ inputs }: { inputs: Schemas.ComponentInput[] }) {
  const t = useT()
  return (
    <section className="flex flex-col gap-2">
      <h3 className="type-meta font-medium text-muted-foreground">{t("components.values")}</h3>
      <dl className="flex flex-col gap-1 text-sm">
        {inputs.map((input) => (
          <div key={keyOf(input)} className="flex flex-wrap gap-x-4">
            <dt className="font-mono break-all text-muted-foreground" translate="no">
              {input.name}
            </dt>
            <dd className="min-w-0 break-all">{reviewOf(t, input)}</dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

/** The states whose review says the same for every kind of input. */
const fixedReview: Partial<Record<Schemas.InputState, (t: Translate) => string>> = {
  empty: (t) => t("components.value.empty"),
  missing: (t) => t("components.value.empty"),
  generated: (t) => t("components.value.generated"),
}

function reviewOf(t: Translate, input: Schemas.ComponentInput) {
  const picked = pickedOf(input)
  if (picked != null) return t("components.value.picked", textOf(picked))
  const fixed = fixedReview[input.state]
  if (fixed) return fixed(t)
  if (input.kind === "secret")
    return t(input.state === "kept" ? "components.value.kept" : "components.value.set")
  const value = input.kind === "flag" ? flagOf(t, input.value) : textOf(input.value)
  return input.state === "default" ? t("components.value.default", value) : value
}

const flagOf = (t: Translate, value: Schemas.InputValue | null | undefined) =>
  t(value === true ? "components.value.on" : "components.value.off")
