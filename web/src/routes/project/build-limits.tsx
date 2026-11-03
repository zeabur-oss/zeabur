import { useForm } from "@tanstack/react-form"

import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import {
  buildCeiling,
  buildLimitProblem,
  buildLimitsChange,
  type BuildLimit,
} from "@/model/project"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card, CardFooter, CardRow, SectionLabel } from "@zeabur-oss/design-system/components/card"
import { Input } from "@zeabur-oss/design-system/components/input"

/** The memory and CPU one build of the project may use; empty uses Builder's default. */
export function BuildLimits({ namespace }: { namespace: Schemas.ProjectNamespace }) {
  return (
    <LimitsForm
      // A new form when the saved limits change, so it starts from them.
      key={JSON.stringify([namespace.buildMemory, namespace.buildCpu])}
      namespace={namespace}
    />
  )
}

function LimitsForm({ namespace }: { namespace: Schemas.ProjectNamespace }) {
  const t = useT()
  const planner = usePlanner(namespace.name)
  const gate = useHintGate("changeBuildLimits", namespace.canAdopt)
  const closed = !gate.open
  const ceiling = buildCeiling(namespace)
  const initial = { buildMemory: namespace.buildMemory ?? "", buildCpu: namespace.buildCpu ?? "" }
  const form = useForm({
    defaultValues: initial,
    onSubmit: ({ value }) =>
      planner.plan({
        title: t("title.buildLimits", namespace.name),
        changes: [buildLimitsChange(value)],
      }),
  })
  const check =
    (kind: BuildLimit) =>
    ({ value }: { value: string }) => {
      const problem = buildLimitProblem(kind, value, namespace)
      if (problem === "invalid") return t("projectSettings.buildInvalid")
      return problem ? t("projectSettings.buildTooLarge", ceiling[kind] ?? "") : undefined
    }
  const rows: { name: BuildLimit; label: string; hint: string }[] = [
    {
      name: "buildMemory",
      label: t("projectSettings.buildMemory"),
      hint:
        t("projectSettings.buildMemoryHint") +
        (ceiling.buildMemory ? t("projectSettings.buildMost", ceiling.buildMemory) : ""),
    },
    {
      name: "buildCpu",
      label: t("projectSettings.buildCpu"),
      hint:
        t("projectSettings.buildCpuHint") +
        (ceiling.buildCpu ? t("projectSettings.buildMost", ceiling.buildCpu) : ""),
    },
  ]
  return (
    <section id="builds" className="scroll-mt-20">
      <SectionLabel>{t("projectSettings.builds")}</SectionLabel>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          if (!closed) void form.handleSubmit()
        }}
      >
        <Card variant="list">
          {rows.map((row) => (
            <form.Field key={row.name} name={row.name} validators={{ onChange: check(row.name) }}>
              {(f) => (
                <CardRow label={row.label} description={row.hint} className="max-sm:items-stretch">
                  <FormField
                    label={row.label}
                    hideLabel
                    errors={f.state.meta.errors}
                    className="w-full sm:w-80"
                  >
                    <Input
                      value={f.state.value}
                      placeholder={t("buildLimits.default")}
                      disabled={closed}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(event) => f.handleChange(event.target.value)}
                      onBlur={f.handleBlur}
                    />
                  </FormField>
                </CardRow>
              )}
            </form.Field>
          ))}
          <form.Subscribe
            selector={(s) =>
              [
                s.values.buildMemory.trim() !== initial.buildMemory ||
                  s.values.buildCpu.trim() !== initial.buildCpu,
                s.canSubmit,
              ] as const
            }
          >
            {([changed, valid]) =>
              closed || changed ? (
                <CardFooter className="justify-end">
                  {closed ? null : (
                    <Button type="button" variant="ghost" size="sm" onClick={() => form.reset()}>
                      {t("draft.discard")}
                    </Button>
                  )}
                  <GateTip gate={gate}>
                    <Button type="submit" size="sm" disabled={!changed || !valid}>
                      {t("action.saveBuildLimits")}
                    </Button>
                  </GateTip>
                </CardFooter>
              ) : null
            }
          </form.Subscribe>
        </Card>
      </form>
      {planner.dialog}
    </section>
  )
}
