import { useQuery } from "@tanstack/react-query"
import { useState } from "react"

import type { Schemas } from "@/api/client"
import { configMapsQuery, objectQuery, projectQuery } from "@/api/queries"
import { secretKeys } from "@/components/data-editor"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"

type Kind = "ConfigMap" | "Secret"
const VARIABLE = /^[A-Za-z_][A-Za-z0-9_]*$/

/** Read a project's shared variables in one container: every key (`envFrom`) or one key
 * (`env[].valueFrom`). */
export default function UseSharedDrawer({
  project,
  service,
  onClose,
}: {
  project: string
  service: Service
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const groups = (useQuery(projectQuery(project)).data?.resources ?? []).filter(
    (r) => r.sharedVariables,
  )
  const containers = service.containers.filter((c) => !c.init)
  const [container, setContainer] = useState(containers[0]?.name ?? "")
  const [chosen, setChosen] = useState(groups[0] ? `${groups[0].kind}/${groups[0].name}` : "")
  const [kind, name] = chosen.split("/") as [Kind, string]
  const [mode, setMode] = useState<"all" | "one">("all")
  const configMaps = useQuery({
    ...configMapsQuery(project),
    enabled: mode === "one" && kind === "ConfigMap",
  })
  const secret = useQuery({
    ...objectQuery(project, { group: "", version: "v1", resource: "secrets", name }),
    enabled: mode === "one" && kind === "Secret",
  })
  const keys =
    kind === "ConfigMap"
      ? Object.keys(configMaps.data?.items.find((c) => c.origin.name === name)?.data ?? {})
      : secretKeys(secret.data?.yaml ?? "")
  const [key, setKey] = useState<string>()
  const pickedKey = key && keys.includes(key) ? key : keys[0]
  const [variable, setVariable] = useState<string>()
  const variableName = (variable ?? pickedKey ?? "").trim()
  const [tried, setTried] = useState(false)
  const own = containers.find((c) => c.name === container)
  const problems = {
    group:
      mode === "all" &&
      own?.importedEnv.some((i) => i.source.kind === kind && i.source.name === name)
        ? t("useShared.already", container)
        : undefined,
    variable:
      mode !== "one"
        ? undefined
        : !VARIABLE.test(variableName)
          ? t("useShared.badVariable")
          : own?.env.some((e) => e.name === variableName)
            ? t("useShared.exists", variableName)
            : undefined,
  }
  const workload = { kind: service.kind, name: service.name }
  const target = containers.length > 1 ? container : undefined
  const change = (): Schemas["Change"] =>
    mode === "all"
      ? { op: "envImport", workload, container: target, kind, name }
      : {
          op: "env",
          workload,
          container: target,
          changes: [
            {
              op: "set",
              name: variableName,
              valueFrom:
                kind === "Secret"
                  ? { secretName: name, key: pickedKey }
                  : { configMapName: name, key: pickedKey },
            },
          ],
        }
  const review = () => {
    setTried(true)
    if (!name || (mode === "one" && !pickedKey) || Object.values(problems).some(Boolean)) return
    step.plan({
      title: t("useShared.planTitle", name, service.name),
      service: workload,
      changes: [change()],
    })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("useShared.title", service.name)}
      description={t("useShared.lead")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" disabled={!name} onClick={review}>
            {t("edit.review")}
          </Button>
        </div>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <FormField label={t("useShared.group")} errors={shown(problems.group)}>
          <Select
            size="sm"
            translate="no"
            value={chosen}
            onValueChange={setChosen}
            items={groups.map((g) => ({
              value: `${g.kind}/${g.name}`,
              label: `${g.name} (${g.kind === "Secret" ? t("shared.secret") : t("shared.plain")})`,
            }))}
          />
        </FormField>
        {containers.length > 1 ? (
          <FormField label={t("tasks.container")}>
            <Select
              size="sm"
              translate="no"
              value={container}
              onValueChange={setContainer}
              items={containers.map((c) => ({ value: c.name, label: c.name }))}
            />
          </FormField>
        ) : null}
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1.5 text-sm font-medium">{t("useShared.which")}</legend>
          <RadioGroup value={mode} onValueChange={(value) => setMode(value as typeof mode)}>
            <RadioOption
              value="all"
              label={t("vars.allKeys")}
              description={t("useShared.allHint")}
            />
            <RadioOption
              value="one"
              label={t("useShared.one")}
              description={t("useShared.oneHint")}
            />
          </RadioGroup>
        </fieldset>
        {mode === "one" ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label={t("useShared.key")}>
              <Select
                size="sm"
                translate="no"
                value={pickedKey ?? ""}
                onValueChange={setKey}
                items={keys.map((k) => ({ value: k, label: k }))}
              />
            </FormField>
            <FormField label={t("useShared.variable")} errors={shown(problems.variable)}>
              <Input
                size="sm"
                mono
                value={variable ?? pickedKey ?? ""}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => setVariable(event.target.value)}
              />
            </FormField>
          </div>
        ) : null}
        <KubeDetails
          facts={[
            [
              t("useShared.kube.writes"),
              mode === "all"
                ? `envFrom: [{${kind === "Secret" ? "secretRef" : "configMapRef"}: {name: ${name}}}]`
                : `env: [{name: ${variableName}, valueFrom: {${kind === "Secret" ? "secretKeyRef" : "configMapKeyRef"}: {name: ${name}, key: ${pickedKey ?? ""}}}}]`,
            ],
            [t("useShared.kube.when"), t("useShared.kube.whenValue")],
          ]}
        />
      </form>
    </PlanDrawer>
  )
}
