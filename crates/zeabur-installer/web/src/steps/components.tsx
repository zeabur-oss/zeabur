import { useId } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"

import type { CatalogComponent, Config, Input as CatalogInput, Preset } from "@/api"
import type { Wizard } from "@/app"
import { Panel, Segmented } from "@/components/bits"
import { MemoryVerdict, memoryAdvice } from "@/components/memory"
import { coded, useT, type Translate } from "@/i18n"
import { memoryFit } from "@/lib/memory"
import { StepPage } from "@/steps/frame"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Field, FieldDescription, FieldLabel } from "@zeabur/design-system/components/field"
import { Input } from "@zeabur/design-system/components/input"
import { LoadingDots } from "@zeabur/design-system/components/spinner"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

const presets: Preset[] = ["minimal", "standard", "secure"]

export function ComponentsStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { config, planned } = wizard
  const choices = wizard.active
  const personal = wizard.use === "personal"
  const all = wizard.server.catalog?.components ?? []
  // A personal install offers only what the catalog tests on KubeSolo.
  const offered = new Set(all.filter((c) => c.personal).map((c) => c.capability))
  const catalog = personal ? all.filter((c) => offered.has(c.capability)) : all
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
        <Button disabled={!plan || wizard.planning} onClick={() => wizard.go("review")}>
          {t("common.continue")}
        </Button>
      }
    >
      {personal ? null : (
        <Panel title={t("components.preset")}>
          <div className="flex flex-col gap-3">
            <Segmented<Preset>
              label={t("components.preset")}
              value={config.preset}
              onChange={(preset) =>
                wizard.setConfig({ ...config, preset, enable: [], disable: [] })
              }
              options={presets.map((value) => ({ value, label: t(`preset.${value}`) }))}
            />
            <p className="text-sm text-secondary">{t(`presetHint.${config.preset}`)}</p>
          </div>
        </Panel>
      )}

      {planned && "error" in planned ? (
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
                supported={personal ? component.personal === true : component.supported}
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
  if (reason === "not part of a personal install") return t("components.notPersonal")
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
  wizard,
}: {
  component: CatalogComponent
  chosen: boolean
  needed: boolean
  mib: number | null
  leftOut: string | undefined
  onToggle: (on: boolean) => void
  supported: boolean
  wizard: Wizard
}) {
  const t = useT()
  const id = useId()
  const disabled = !supported || needed
  const required = component.inputs.filter((input) => input.required)
  const optional = component.inputs.filter((input) => !input.required)
  const unsupported = component.distributions?.length
    ? t("components.runsOn", t("common.list", component.distributions))
    : leftOut
      ? leftOutReason(t, leftOut)
      : t("components.unsupported")
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
        <CollapsibleContent>
          <p className="mt-1 ml-7.5 text-sm text-secondary">{component.security}</p>
        </CollapsibleContent>
        {chosen && required.length ? (
          <div className="mt-2 ml-7.5 flex flex-col gap-4 rounded-lg border border-v2-border p-4">
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
  const value = input.secret
    ? (wizard.secrets[component]?.[input.name] ?? "")
    : show(choices.inputs[component]?.[input.name])
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
  const notes = [
    input.description,
    Array.isArray(input.default) ? t("components.list") : "",
    input.generated ? t("components.generated") : "",
    input.secret ? t("components.secret") : "",
  ].filter(Boolean)
  return (
    <Field>
      <FieldLabel>
        <span className="font-mono">{input.name}</span>
        {input.required ? (
          <span className="ml-2 text-xs font-normal text-tertiary">{t("components.required")}</span>
        ) : null}
      </FieldLabel>
      <Input
        type={input.secret ? "password" : "text"}
        defaultValue={value}
        placeholder={show(input.default)}
        mono
        autoComplete="off"
        className="max-w-md"
        onBlur={(event) => set(event.target.value)}
      />
      <FieldDescription>{notes.join(" ")}</FieldDescription>
    </Field>
  )
}
