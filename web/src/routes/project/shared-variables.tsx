import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { lazy, Suspense, useState } from "react"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGates } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { parseDotenv } from "@/model/env-edit"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import { Input } from "@zeabur/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Textarea } from "@zeabur/design-system/components/textarea"

const DataEditor = lazy(() =>
  import("@/components/data-editor").then((m) => ({ default: m.DataEditor })),
)

const SHARED_LABEL = "zeabur.com/shared-variables"
const SHOWN = 20
const NAME = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$/
const KEY = /^[-._a-zA-Z0-9]+$/

type Kind = "ConfigMap" | "Secret"

/** The project's ConfigMaps and Secrets labelled as shared variables, and who reads them. */
export function SharedVariablesSection({ project }: { project: string }) {
  const t = useT()
  const document = useQuery(projectQuery(project))
  const groups = (document.data?.resources ?? []).filter((r) => r.sharedVariables)
  const [all, setAll] = useState(false)
  const [adding, setAdding] = useState(false)
  const [editing, setEditing] = useState<{ kind: Kind; name: string }>()
  const gates = useGates(project)
  const gate = gates("changeConfig")
  const shown = all ? groups : groups.slice(0, SHOWN)
  return (
    <section id="shared-variables">
      <SectionLabel>{t("shared.title")}</SectionLabel>
      <p className="mb-3 type-meta text-muted-foreground">{t("shared.lead")}</p>
      <GateNote gate={gate} />
      <Card variant="list">
        {shown.map((group) => (
          <CardRow
            key={group.id}
            label={
              <span className="flex flex-wrap items-center gap-2">
                <span className="type-mono" translate="no">
                  {group.name}
                </span>
                <Badge variant="secondary">
                  {group.kind === "Secret" ? t("shared.secret") : t("shared.plain")}
                </Badge>
              </span>
            }
            description={<UsedBy project={project} resource={group} />}
          >
            <span className="flex items-center gap-2">
              {group.keyCount != null ? (
                <span className="type-meta whitespace-nowrap text-muted-foreground">
                  {t("vars.keys", group.keyCount)}
                </span>
              ) : null}
              <Button
                variant="muted"
                size="icon-xs"
                aria-label={t("edit.edit", group.name)}
                title={t("edit.edit", group.name)}
                onClick={() => setEditing({ kind: group.kind as Kind, name: group.name })}
              >
                <PencilSimpleIcon aria-hidden="true" />
              </Button>
            </span>
          </CardRow>
        ))}
        {groups.length > SHOWN && !all ? (
          <CardRow label="">
            <Button variant="ghost" size="sm" onClick={() => setAll(true)}>
              {t("shared.showAll", groups.length)}
            </Button>
          </CardRow>
        ) : null}
        <CardRow
          label={groups.length === 0 ? t("shared.none") : t("shared.another")}
          description={groups.length === 0 ? t("shared.noneHint") : undefined}
        >
          <GateTip gate={gate}>
            <Button variant="outline" size="sm" onClick={() => setAdding(true)}>
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("shared.add")}
            </Button>
          </GateTip>
        </CardRow>
      </Card>
      {adding ? (
        <AddDrawer
          project={project}
          taken={
            new Set(
              (document.data?.resources ?? [])
                .filter((r) => r.kind === "ConfigMap" || r.kind === "Secret")
                .map((r) => `${r.kind}/${r.name}`),
            )
          }
          onClose={() => setAdding(false)}
        />
      ) : null}
      {editing ? (
        <Suspense fallback={null}>
          <DataEditor
            project={project}
            kind={editing.kind}
            name={editing.name}
            onClose={() => setEditing(undefined)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}

function UsedBy({ project, resource }: { project: string; resource: Schemas["Resource"] }) {
  const t = useT()
  const owners = resource.attribution.owners
  if (owners.length === 0) return <span>{t("shared.unused")}</span>
  return (
    <span className="flex flex-wrap items-center gap-x-1.5">
      {t("shared.usedBy")}
      {owners.map((owner, i) => (
        <span key={`${owner.kind}/${owner.name}`}>
          <Link
            {...serviceLink(project, owner, { tab: "variables" })}
            className="outline-none hover:underline focus-visible:underline"
            translate="no"
          >
            {owner.name}
          </Link>
          {i < owners.length - 1 ? "," : ""}
        </span>
      ))}
    </span>
  )
}

function AddDrawer({
  project,
  taken,
  onClose,
}: {
  project: string
  taken: Set<string>
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const [name, setName] = useState("shared")
  const [kind, setKind] = useState<Kind>("Secret")
  const [text, setText] = useState("")
  const [tried, setTried] = useState(false)
  const parsed = parseDotenv(text)
  const badKey = parsed.entries.find((e) => !KEY.test(e.name))
  const problems = {
    name: !NAME.test(name)
      ? t("shared.badName")
      : taken.has(`${kind}/${name}`)
        ? t("shared.taken", kind, name)
        : undefined,
    values:
      parsed.entries.length === 0
        ? t("shared.needValues")
        : parsed.unread[0] !== undefined
          ? t("shared.unread", parsed.unread[0])
          : badKey
            ? t("shared.badKey", badKey.name)
            : undefined,
  }
  const values = Object.fromEntries(parsed.entries.map((e) => [e.name, e.value]))
  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean)) return
    step.plan({
      title: t("shared.planTitle", name),
      changes: [
        {
          op: "create",
          object: {
            apiVersion: "v1",
            kind,
            metadata: { name, labels: { [SHARED_LABEL]: "true" } },
            ...(kind === "Secret" ? { type: "Opaque", stringData: values } : { data: values }),
          } as unknown as Record<string, never>,
        },
      ],
    })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("shared.addTitle")}
      description={t("shared.addLead")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FormField label={t("shared.name")} errors={shown(problems.name)}>
          <Input
            size="sm"
            mono
            value={name}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setName(event.target.value.trim())}
          />
        </FormField>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1.5 text-sm font-medium">{t("shared.kind")}</legend>
          <RadioGroup value={kind} onValueChange={(value) => setKind(value as Kind)}>
            <RadioOption
              value="Secret"
              label={t("shared.secret")}
              description={t("shared.secretHint")}
            />
            <RadioOption
              value="ConfigMap"
              label={t("shared.plain")}
              description={t("shared.plainHint")}
            />
          </RadioGroup>
        </fieldset>
        <FormField
          label={t("shared.values")}
          hint={t("shared.valuesHint")}
          errors={shown(problems.values)}
        >
          <Textarea
            mono
            rows={6}
            value={text}
            placeholder={"APP_URL=https://example.com\nMAILER_TOKEN=..."}
            spellCheck={false}
            onChange={(event) => setText(event.target.value)}
          />
        </FormField>
        <KubeDetails
          facts={[
            [t("shared.kube.object"), `${kind} ${name}`],
            [t("shared.kube.label"), `${SHARED_LABEL}: "true"`],
            [t("shared.kube.read"), t("shared.kube.readValue")],
          ]}
        />
      </ReviewForm>
    </PlanDrawer>
  )
}
