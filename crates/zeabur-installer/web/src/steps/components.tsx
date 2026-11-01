import { useId, useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"

import {
  personalDistribution,
  runsOn,
  titleOf,
  type CatalogComponent,
  type Config,
  type Input as CatalogInput,
} from "@/api"
import type { Wizard } from "@/app"
import { Panel } from "@/components/bits"
import { MemoryVerdict, memoryAdvice } from "@/components/memory"
import { coded, named, useT, type Translate } from "@/i18n"
import {
  inputKind,
  lacksInput,
  velero,
  veleroForm,
  veleroInputs,
  veleroKeys,
  type InputKind,
} from "@/lib/inputs"
import { memoryFit } from "@/lib/memory"
import { StepPage } from "@/steps/frame"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Checkbox } from "@zeabur-oss/design-system/components/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { Field, FieldDescription, FieldLabel } from "@zeabur-oss/design-system/components/field"
import { Input } from "@zeabur-oss/design-system/components/input"
import { LoadingDots } from "@zeabur-oss/design-system/components/spinner"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"

export function ComponentsStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { planned } = wizard
  const choices = wizard.active
  const personal = wizard.use === "personal"
  const distribution = personal ? personalDistribution : wizard.config.distribution
  const catalog = (wizard.server.catalog?.components ?? []).filter((c) =>
    c.testedOn.includes(distribution),
  )
  const hostMib = wizard.server.facts?.memoryMib ?? Infinity
  const tooSmall = (c: CatalogComponent) => personal && hostMib < (c.personalMinMemoryMib ?? 0)
  const plan = planned && "plan" in planned ? planned.plan : null
  const fit = plan ? memoryFit(plan, catalog) : null
  const why = new Map(wizard.shown?.components.map((c) => [c.id, c.why]))
  const measured = new Map(wizard.shown?.components.map((c) => [c.id, c.idleMemoryMib]))
  const chosen = (id: string) =>
    choices.enable.includes(id) || (why.has(id) && !choices.disable.includes(id))
  const needs = new Map(catalog.map((c) => [c.id, c.needs ?? []]))
  const needed = new Set(
    [...why.keys()].filter((id) => chosen(id)).flatMap((id) => needs.get(id) ?? []),
  )
  const groups = new Map<string, CatalogComponent[]>()
  for (const component of catalog) {
    groups.set(component.capability, [...(groups.get(component.capability) ?? []), component])
  }
  const toggle = (id: string, on: boolean) => {
    const enable = choices.enable.filter((x) => x !== id)
    const disable = choices.disable.filter((x) => x !== id)
    wizard.setChoices({
      enable: on ? [...enable, id] : enable,
      disable: on ? disable : [...disable, id],
      inputs: choices.inputs,
    })
  }
  const advice = fit ? memoryAdvice(t, fit) : ""
  const given = { inputs: choices.inputs, secrets: wizard.secrets }
  // The plan names the first missing input in configuration terms; the rows ask for them instead.
  const incomplete = catalog.filter((c) => chosen(c.id) && lacksInput(c, given))
  return (
    <StepPage
      title={t("components.title")}
      description={t(personal ? "components.personalDescription" : "components.description")}
      back={
        <Button variant="outline" onClick={() => wizard.go(personal ? "host" : "access")}>
          {t("common.back")}
        </Button>
      }
      next={
        <>
          {incomplete.length ? (
            <span className="text-xs text-warn-strong">
              {t(
                "components.incomplete",
                t(
                  "common.list",
                  incomplete.map((c) => c.displayName),
                ),
              )}
            </span>
          ) : null}
          <Button
            disabled={!plan || wizard.planning || incomplete.length > 0}
            onClick={() => wizard.go("review")}
          >
            {t("common.continue")}
          </Button>
        </>
      }
    >
      {planned && "error" in planned && !incomplete.length ? (
        <Alert variant="destructive">
          <AlertTitle>{t("common.error")}</AlertTitle>
          <AlertDescription>{planned.error}</AlertDescription>
        </Alert>
      ) : null}

      <Panel
        title={t("step.components")}
        flush
        sticky
        action={
          <span className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-secondary">
            {wizard.planning ? <LoadingDots /> : null}
            {plan ? <span>{t("components.chosen", plan.components.length)}</span> : null}
            {fit ? <MemoryVerdict fit={fit} /> : null}
          </span>
        }
      >
        {advice ? <p className="px-5 py-3 text-sm text-secondary">{advice}</p> : null}
        {[...groups].map(([capability, components]) => (
          <div key={capability} className="flex flex-col">
            <h3 className="px-5 pt-3 pb-0.5 text-xs font-medium text-tertiary">
              {coded(t, `capability.${capability}`, [], capability)}
            </h3>
            {components.map((component) => (
              <ComponentRow
                key={component.id}
                component={component}
                chosen={chosen(component.id)}
                needed={why.get(component.id) === "dependency" || needed.has(component.id)}
                mib={component.idleMemoryMib ?? measured.get(component.id) ?? null}
                leftOut={plan?.leftOut[component.id]}
                onToggle={(on) => toggle(component.id, on)}
                supported={runsOn(component, distribution) && !tooSmall(component)}
                tooSmall={tooSmall(component)}
                distribution={distribution}
                wizard={wizard}
              />
            ))}
          </div>
        ))}
      </Panel>
    </StepPage>
  )
}

/** The plan's reason for leaving a component out, in the words the components page uses. */
export function leftOutReason(t: Translate, reason: string): string {
  const needsUntested = /^needs (.+), not tested on (.+)$/.exec(reason)
  if (needsUntested)
    return t("components.needsUntested", needsUntested[1]!, titleOf(needsUntested[2]!))
  const untested = /^not tested on (.+)$/.exec(reason)
  if (untested) return t("components.untested", titleOf(untested[1]!))
  const memory = /^needs (\d+) MiB of memory$/.exec(reason)
  if (memory) return t("components.needsMemory", Number(memory[1]))
  const only = /^runs on (.+) only$/.exec(reason)
  if (only) return t("components.runsOn", only[1]!)
  const profile = /^unsupported on (.+)$/.exec(reason)
  if (profile) return t("components.unsupportedOn", profile[1]!)
  return reason
}

function ComponentRow({
  component,
  chosen,
  needed,
  mib,
  leftOut,
  onToggle,
  supported,
  tooSmall,
  distribution,
  wizard,
}: {
  component: CatalogComponent
  chosen: boolean
  needed: boolean
  mib: number | null
  leftOut: string | undefined
  onToggle: (on: boolean) => void
  supported: boolean
  tooSmall: boolean
  distribution: string
  wizard: Wizard
}) {
  const t = useT()
  const id = useId()
  const disabled = !supported || needed
  const form = veleroForm(component)
  const note = named(t, `note.${component.id}`, "")
  const asked = component.inputs.filter(
    (input) => !input.filled && !(form && veleroInputs.includes(input.name)),
  )
  const required = asked.filter((input) => input.required)
  const optional = asked.filter((input) => !input.required)
  const unsupported = tooSmall
    ? t("components.needsMemory", component.personalMinMemoryMib ?? 0)
    : component.distributions.length
      ? t("components.runsOn", t("common.list", component.distributions))
      : leftOut
        ? leftOutReason(t, leftOut)
        : t("components.unsupported", titleOf(distribution))
  return (
    <div className="px-5 py-2">
      <Collapsible className="flex flex-col">
        <div className="flex min-h-7 items-center gap-3">
          <Checkbox
            id={id}
            checked={chosen}
            disabled={disabled}
            onCheckedChange={(on) => onToggle(on)}
          />
          <span className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
            <label htmlFor={id} className="text-sm font-medium text-main">
              {component.displayName}
            </label>
            {!supported ? <Badge variant="secondary">{unsupported}</Badge> : null}
            {needed ? <Badge variant="secondary">{t("components.needed")}</Badge> : null}
            {component.privileged ? (
              <Tooltip content={t("components.privilegedHint")}>
                <Badge variant="warn" render={<button type="button" />}>
                  {t("components.privileged")}
                </Badge>
              </Tooltip>
            ) : null}
          </span>
          {mib ? (
            <span className="shrink-0 text-xs text-tertiary tabular-nums">
              {t("common.mib", mib)}
            </span>
          ) : null}
          <CollapsibleTrigger
            aria-label={t("components.details", component.displayName)}
            render={<Button variant="ghost" size="icon-xs" />}
          >
            <CaretDownIcon aria-hidden="true" />
          </CollapsibleTrigger>
        </div>
        {note ? <p className="ml-7.5 text-xs text-secondary">{note}</p> : null}
        <CollapsibleContent>
          <p className="mt-1 ml-7.5 text-sm text-secondary">{component.security}</p>
        </CollapsibleContent>
        {chosen && (form || required.length) ? (
          <div className="mt-2 ml-7.5 flex flex-col gap-4 rounded-lg border border-v2-border p-4">
            {form ? <VeleroFields wizard={wizard} /> : null}
            {required.map((input) => (
              <InputField key={input.name} component={component.id} input={input} wizard={wizard} />
            ))}
          </div>
        ) : null}
        {chosen && optional.length ? (
          <div className="mt-1 ml-7.5 text-xs">
            <Collapsible>
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                {t("components.customise", optional.length)}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="mt-2 flex flex-col gap-4 rounded-lg border border-v2-border p-4">
                  {optional.map((input) => (
                    <InputField
                      key={input.name}
                      component={component.id}
                      input={input}
                      wizard={wizard}
                    />
                  ))}
                </div>
              </CollapsibleContent>
            </Collapsible>
          </div>
        ) : null}
      </Collapsible>
    </div>
  )
}

function show(value: unknown): string {
  if (Array.isArray(value)) return `[${value.join(", ")}]`
  return value === undefined || value === null ? "" : String(value)
}

/** The S3 location Velero writes to, asked as the panel's backup location form asks it. gui.rs
 * makes the `credentials` file from the two keys. */
function VeleroFields({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const choices = wizard.active
  const given = choices.inputs[velero] ?? {}
  const secrets = wizard.secrets[velero] ?? {}
  const setInput = (name: string, text: string) => {
    const values = { ...choices.inputs[velero] }
    if (text.trim() === "") delete values[name]
    else values[name] = text.trim()
    wizard.setChoices({
      enable: choices.enable,
      disable: choices.disable,
      inputs: { ...choices.inputs, [velero]: values },
    })
  }
  const [keyId, secretKey] = veleroKeys
  return (
    <>
      <Setting
        label={t("velero.endpoint")}
        help={t("velero.endpointHint")}
        kind="url"
        required
        value={show(given["s3-url"])}
        placeholder="https://<account>.r2.cloudflarestorage.com"
        onChange={(text) => setInput("s3-url", text)}
      />
      <Setting
        label={t("velero.bucket")}
        kind="text"
        required
        value={show(given.bucket)}
        placeholder="zeabur-backups"
        onChange={(text) => setInput("bucket", text)}
      />
      <Setting
        label={t("velero.region")}
        help={t("velero.regionHint")}
        kind="text"
        value={show(given.region)}
        placeholder="us-east-1"
        onChange={(text) => setInput("region", text)}
      />
      <Setting
        label={t("velero.accessKey")}
        kind="text"
        required
        value={secrets[keyId] ?? ""}
        onChange={(text) => wizard.setSecret(velero, keyId, text)}
      />
      <Setting
        label={t("velero.secretKey")}
        help={t("components.secret")}
        kind="secret"
        required
        value={secrets[secretKey] ?? ""}
        onChange={(text) => wizard.setSecret(velero, secretKey, text)}
      />
    </>
  )
}

/** One field. It keeps what the person types and passes each change on. */
function Setting({
  label,
  help,
  kind,
  required,
  value,
  placeholder,
  onChange,
}: {
  label: string
  help?: string
  kind: InputKind
  required?: boolean
  value: string
  placeholder?: string
  onChange: (text: string) => void
}) {
  const t = useT()
  const id = useId()
  const [text, setText] = useState(value)
  const change = (next: string) => {
    setText(next)
    onChange(next)
  }
  const name = (
    <>
      {label}
      {required ? (
        <span className="ml-2 text-xs font-normal text-tertiary">{t("components.required")}</span>
      ) : null}
    </>
  )
  if (kind === "boolean") {
    return (
      <Field>
        <div className="flex items-center gap-2">
          <Checkbox
            id={id}
            checked={text === "true"}
            onCheckedChange={(on) => change(on ? "true" : "false")}
          />
          <label htmlFor={id} className="text-sm font-medium text-main">
            {name}
          </label>
        </div>
        {help ? <FieldDescription className="ml-6">{help}</FieldDescription> : null}
      </Field>
    )
  }
  return (
    <Field>
      <FieldLabel>{name}</FieldLabel>
      {kind === "multiline" ? (
        <Textarea
          value={text}
          placeholder={placeholder}
          mono
          autoComplete="off"
          className="max-w-md"
          onChange={(event) => change(event.target.value)}
        />
      ) : (
        <Input
          type={kind === "secret" ? "password" : kind === "url" ? "url" : "text"}
          value={text}
          placeholder={placeholder}
          mono
          autoComplete="off"
          className="max-w-md"
          onChange={(event) => change(event.target.value)}
        />
      )}
      {help ? <FieldDescription>{help}</FieldDescription> : null}
    </Field>
  )
}

/** What a person types, as the configuration holds it: `[a, b]` is a list, digits a number. */
function parse(text: string): unknown {
  const trimmed = text.trim()
  if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
    return trimmed
      .slice(1, -1)
      .split(",")
      .map((item) => item.trim().replace(/^["']|["']$/g, ""))
      .filter(Boolean)
  }
  if (trimmed === "true" || trimmed === "false") return trimmed === "true"
  if (/^-?\d+$/.test(trimmed)) return Number(trimmed)
  return trimmed
}

function InputField({
  component,
  input,
  wizard,
}: {
  component: string
  input: CatalogInput
  wizard: Wizard
}) {
  const t = useT()
  const choices = wizard.active
  const kind = inputKind(component, input)
  const value = input.secret
    ? (wizard.secrets[component]?.[input.name] ?? "")
    : show(choices.inputs[component]?.[input.name] ?? (kind === "boolean" ? input.default : ""))
  const set = (text: string) => {
    if (input.secret) {
      wizard.setSecret(component, input.name, text)
      return
    }
    const values = { ...choices.inputs[component] }
    if (text.trim() === "") delete values[input.name]
    else values[input.name] = parse(text)
    const inputs: Config["inputs"] = { ...choices.inputs, [component]: values }
    wizard.setChoices({ enable: choices.enable, disable: choices.disable, inputs })
  }
  const key = `${component}.${input.name}`
  const help = named(t, `inputHelp.${key}`, input.description)
  const notes = [
    help,
    Array.isArray(input.default) ? t("components.list") : "",
    input.generated ? t("components.generated") : "",
    input.secret ? t("components.secret") : "",
  ].filter(Boolean)
  return (
    <Setting
      label={named(t, `input.${key}`, input.name)}
      help={notes.join(" ")}
      kind={kind}
      required={input.required}
      value={value}
      placeholder={kind === "boolean" ? undefined : show(input.default)}
      onChange={set}
    />
  )
}
