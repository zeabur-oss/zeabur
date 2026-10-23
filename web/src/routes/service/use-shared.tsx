import { useQuery } from "@tanstack/react-query"
import { useState } from "react"

import type { Schemas } from "@/api/client"
import { configMapsQuery, objectQuery, projectQuery } from "@/api/queries"
import { secretKeys } from "@/components/data-editor"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import type { Service } from "@/model/service"
import { ContainerField } from "@/routes/service/container-field"
import { Input } from "@zeabur/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"

type Kind = "ConfigMap" | "Secret"
const VARIABLE = /^[A-Za-z_][A-Za-z0-9_]*$/

type Container = Schemas["Service"]["containers"][number]

/** The keys of a shared group, read only when one key is to be chosen. */
function useKeys(project: string, kind: Kind, name: string, wanted: boolean): string[] {
  const configMaps = useQuery({
    ...configMapsQuery(project),
    enabled: wanted && kind === "ConfigMap",
  })
  const secret = useQuery({
    ...objectQuery(project, { group: "", version: "v1", resource: "secrets", name }),
    enabled: wanted && kind === "Secret",
  })
  if (kind === "Secret") return secretKeys(secret.data?.yaml ?? "")
  return Object.keys(configMaps.data?.items.find((c) => c.origin.name === name)?.data ?? {})
}

function groupProblem(t: Translate, own: Container | undefined, kind: Kind, name: string) {
  return own?.importedEnv.some((i) => i.source.kind === kind && i.source.name === name)
    ? t("useShared.already", own.name)
    : undefined
}

function variableProblem(t: Translate, own: Container | undefined, variable: string) {
  if (!VARIABLE.test(variable)) return t("useShared.badVariable")
  return own?.env.some((e) => e.name === variable) ? t("useShared.exists", variable) : undefined
}

interface Asked {
  mode: "all" | "one"
  own: Container | undefined
  workload: Schemas["WorkloadRef"]
  container: string | undefined
  kind: Kind
  name: string
  variable: string
  key: string | undefined
}

/** The problems with what is asked, and the change once there are none. */
function planned(t: Translate, asked: Asked) {
  const { mode, own, workload, container, kind, name, variable, key } = asked
  if (mode === "all") {
    const problems = { group: groupProblem(t, own, kind, name), variable: undefined }
    const change: Schemas["Change"] = { op: "envImport", workload, container, kind, name }
    return { problems, change: name && !problems.group ? change : undefined }
  }
  const problems = { group: undefined, variable: variableProblem(t, own, variable) }
  const valueFrom = kind === "Secret" ? { secretName: name, key } : { configMapName: name, key }
  const change: Schemas["Change"] = {
    op: "env",
    workload,
    container,
    changes: [{ op: "set", name: variable, valueFrom }],
  }
  return { problems, change: name && key && !problems.variable ? change : undefined }
}

/** The field the plan writes, as the Kubernetes details show it. */
function writes(
  mode: "all" | "one",
  kind: Kind,
  name: string,
  variable: string,
  key: string | undefined,
): string {
  const secret = kind === "Secret"
  if (mode === "all")
    return `envFrom: [{${secret ? "secretRef" : "configMapRef"}: {name: ${name}}}]`
  const ref = secret ? "secretKeyRef" : "configMapKeyRef"
  return `env: [{name: ${variable}, valueFrom: {${ref}: {name: ${name}, key: ${key ?? ""}}}}]`
}

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
  const [chosen, setChosen] = useState(groupId(groups[0]))
  const [kind, name] = chosen.split("/") as [Kind, string]
  const [mode, setMode] = useState<"all" | "one">("all")
  const one = useOneKey(project, kind, name, mode === "one")
  const [tried, setTried] = useState(false)
  const workload = { kind: service.kind, name: service.name }
  const { problems, change } = planned(t, {
    mode,
    own: containers.find((c) => c.name === container),
    workload,
    container: containers.length > 1 ? container : undefined,
    kind,
    name,
    variable: one.variable,
    key: one.key,
  })
  const review = () => {
    setTried(true)
    if (!change) return
    step.plan({
      title: t("useShared.planTitle", name, service.name),
      service: workload,
      changes: [change],
    })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("useShared.title", service.name)}
      description={t("useShared.lead")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} disabled={!name} />}
    >
      <ReviewForm onReview={review}>
        <GroupField
          groups={groups}
          value={chosen}
          onChange={setChosen}
          errors={shown(problems.group)}
        />
        <ContainerField containers={containers} value={container} onChange={setContainer} />
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
        {mode === "one" ? <OneKeyFields one={one} errors={shown(problems.variable)} /> : null}
        <KubeDetails
          facts={[
            [t("useShared.kube.writes"), writes(mode, kind, name, one.variable, one.key)],
            [t("useShared.kube.when"), t("useShared.kube.whenValue")],
          ]}
        />
      </ReviewForm>
    </PlanDrawer>
  )
}

type Group = Schemas["Resource"]

const groupId = (group: Group | undefined) => (group ? `${group.kind}/${group.name}` : "")

function GroupField({
  groups,
  value,
  onChange,
  errors,
}: {
  groups: Group[]
  value: string
  onChange: (id: string) => void
  errors: string[]
}) {
  const t = useT()
  return (
    <FormField label={t("useShared.group")} errors={errors}>
      <Select
        size="sm"
        translate="no"
        value={value}
        onValueChange={onChange}
        items={groups.map((g) => ({
          value: groupId(g),
          label: `${g.name} (${g.kind === "Secret" ? t("shared.secret") : t("shared.plain")})`,
        }))}
      />
    </FormField>
  )
}

/** One key of a group, and the variable it goes in: the key's name until another is typed. */
function useOneKey(project: string, kind: Kind, name: string, wanted: boolean) {
  const keys = useKeys(project, kind, name, wanted)
  const [key, setKey] = useState<string>()
  const [variable, setVariable] = useState<string>()
  const picked = key && keys.includes(key) ? key : keys[0]
  return {
    keys,
    key: picked,
    setKey,
    variable: (variable ?? picked ?? "").trim(),
    setVariable,
  }
}

function OneKeyFields({ one, errors }: { one: ReturnType<typeof useOneKey>; errors: string[] }) {
  const t = useT()
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <FormField label={t("useShared.key")}>
        <Select
          size="sm"
          translate="no"
          value={one.key ?? ""}
          onValueChange={one.setKey}
          items={one.keys.map((k) => ({ value: k, label: k }))}
        />
      </FormField>
      <FormField label={t("useShared.variable")} errors={errors}>
        <Input
          size="sm"
          mono
          value={one.variable}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => one.setVariable(event.target.value)}
        />
      </FormField>
    </div>
  )
}
