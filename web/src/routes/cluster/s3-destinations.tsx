import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import WarningIcon from "~icons/ph/warning"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import {
  refresh as refreshQuery,
  s3DestinationsPlanQuery,
  s3DestinationsQuery,
} from "@/api/queries"
import { PlanDiffs, PlanFrame } from "@/components/cluster-plan"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { FormDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { useT } from "@/i18n"
import type { Gate } from "@/model/rights"
import {
  type Destination,
  type DestinationForm,
  type DestinationProblem,
  destinationForm,
  destinationProblems,
  saveRequest,
} from "@/model/s3-destinations"
import { Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

type Request = Schemas.S3DestinationsPlanRequest
type Planned = { request: Request; title: string; destructive?: boolean }

const refresh = () => {
  void refreshQuery(s3DestinationsQuery)
}

/** The S3 destinations backup forms offer. Each is a ConfigMap and a Secret of its own
 * namespace; a plan copies the keys into a project, so no backup reads them here. New keys go to
 * the copies in the same plan. */
export function S3DestinationsSection() {
  const t = useT()
  const data = useQuery(s3DestinationsQuery).data
  const gate = useHintGate("changeS3Destinations", data?.canChange)
  const [editing, setEditing] = useState<Destination | "new">()
  const [plan, setPlan] = useState<Planned>()
  if (!data) return null
  const review = (planned: Planned) => {
    setEditing(undefined)
    setPlan(planned)
  }
  return (
    <Section
      id="s3-destinations"
      title={t("s3d.title")}
      count={data.items.length > 0 ? data.items.length : undefined}
      action={
        <GateTip gate={gate}>
          <Button size="xs" variant="outline" onClick={() => setEditing("new")}>
            {t("s3d.add")}
          </Button>
        </GateTip>
      }
    >
      <p className="type-meta text-muted-foreground">{t("s3d.body")}</p>
      {data.items.length > 0 ? (
        <ul className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
          {data.items.map((d) => (
            <DestinationRow
              key={d.name}
              destination={d}
              gate={gate}
              onEdit={() => setEditing(d)}
              onRemove={() =>
                review({
                  request: { op: "remove", name: d.name },
                  title: t("s3d.plan.remove", d.name),
                  destructive: true,
                })
              }
            />
          ))}
        </ul>
      ) : null}
      {editing ? (
        <DestinationEditor
          editing={editing === "new" ? undefined : editing}
          taken={data.items.map((d) => d.name)}
          onCancel={() => setEditing(undefined)}
          onReview={(request) => review({ request, title: t("s3d.plan.save", request.name) })}
        />
      ) : null}
      {plan ? <DestinationsPlan planned={plan} onClose={() => setPlan(undefined)} /> : null}
    </Section>
  )
}

function DestinationRow({
  destination,
  gate,
  onEdit,
  onRemove,
}: {
  destination: Destination
  gate: Gate
  onEdit: () => void
  onRemove: () => void
}) {
  const t = useT()
  const where = [destination.bucket, destination.prefix].filter(Boolean).join("/")
  return (
    <li className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1 px-4 py-2">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate type-body font-medium" translate="no">
          {destination.name}
        </span>
        <span className="truncate type-meta text-muted-foreground" translate="no">
          {[
            where,
            destination.endpoint,
            destination.region,
            destination.caCert ? t("s3d.ownCa") : "",
          ]
            .filter(Boolean)
            .join(t("common.sep"))}
        </span>
      </div>
      <GateTip gate={gate}>
        <Button size="xs" variant="ghost" onClick={onEdit}>
          {t("clusterBackups.edit")}
        </Button>
      </GateTip>
      <GateTip gate={gate}>
        <Button size="xs" variant="ghost" onClick={onRemove}>
          {t("projectSettings.removeAction")}
        </Button>
      </GateTip>
    </li>
  )
}

type Label =
  | "clusterBackups.endpoint"
  | "clusterBackups.bucket"
  | "clusterBackups.folder"
  | "clusterBackups.region"
  | "clusterBackups.accessKey"

const fields: {
  key: Exclude<keyof DestinationForm, "caCert">
  label: Label
  problem?: DestinationProblem
  placeholder?: string
}[] = [
  {
    key: "endpoint",
    label: "clusterBackups.endpoint",
    problem: "endpoint",
    placeholder: "https://<account>.r2.cloudflarestorage.com",
  },
  { key: "bucket", label: "clusterBackups.bucket", problem: "bucket" },
  { key: "prefix", label: "clusterBackups.folder", problem: "prefix", placeholder: "zeabur" },
  { key: "region", label: "clusterBackups.region", placeholder: "auto" },
  { key: "accessKeyId", label: "clusterBackups.accessKey", problem: "keys" },
]

const ownProblems = {
  name: "projects.problem.invalid",
  taken: "s3d.problem.taken",
} as const

const problemText = (p: DestinationProblem) =>
  p === "name" || p === "taken" ? ownProblems[p] : (`clusterBackups.problem.${p}` as const)

function DestinationEditor({
  editing,
  taken,
  onCancel,
  onReview,
}: {
  editing?: Destination
  taken: string[]
  onCancel: () => void
  onReview: (request: Extract<Request, { op: "save" }>) => void
}) {
  const t = useT()
  const [form, setForm] = useState(() => destinationForm(editing))
  const [tried, setTried] = useState(false)
  const problems = destinationProblems(form, taken, editing !== undefined)
  const errors = (...these: (DestinationProblem | undefined)[]) =>
    these
      .filter((p): p is DestinationProblem => !!p && tried && problems.includes(p))
      .map((p) => t(problemText(p)))
  const input = (key: keyof DestinationForm, placeholder?: string) => (
    <Input
      size="sm"
      mono
      autoFocus={key === (editing ? "endpoint" : "name")}
      value={form[key]}
      placeholder={placeholder}
      autoComplete="off"
      spellCheck={false}
      disabled={key === "name" && editing !== undefined}
      onChange={(event) => setForm((f) => ({ ...f, [key]: event.target.value }))}
    />
  )
  const review = () => {
    setTried(true)
    if (problems.length === 0) onReview(saveRequest(form))
  }
  return (
    <FormDrawer
      onClose={onCancel}
      title={editing ? t("s3d.edit", editing.name) : t("s3d.add")}
      footer={<ReviewFooter onCancel={onCancel} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FormField label={t("vars.name")} errors={errors("name", "taken")}>
          {input("name", "r2-backups")}
        </FormField>
        {fields.map((f) => (
          <FormField key={f.key} label={t(f.label)} errors={errors(f.problem)}>
            {input(f.key, f.placeholder)}
          </FormField>
        ))}
        <FormField
          label={t("clusterBackups.secretKey")}
          hint={editing ? t("clusterBackups.keepKeys") : undefined}
        >
          <Input
            size="sm"
            mono
            type="password"
            value={form.secretAccessKey}
            autoComplete="new-password"
            onChange={(event) => setForm((f) => ({ ...f, secretAccessKey: event.target.value }))}
          />
        </FormField>
        <CaField value={form.caCert} onChange={(caCert) => setForm((f) => ({ ...f, caCert }))} />
        {/* The footer's button is outside the form, so Enter needs one in it. */}
        <button type="submit" hidden />
      </ReviewForm>
    </FormDrawer>
  )
}

/** The CA certificates of a store whose certificate a private CA signs, such as a self-hosted
 * MinIO's. */
export function CaField({
  value,
  hint,
  onChange,
}: {
  value: string
  hint?: string
  onChange: (value: string) => void
}) {
  const t = useT()
  return (
    <FormField label={t("s3d.caCert")} hint={hint ?? t("s3d.caCertHint")}>
      <Textarea
        rows={3}
        size="sm"
        mono
        value={value}
        placeholder="-----BEGIN CERTIFICATE-----"
        autoComplete="off"
        onChange={(event) => onChange(event.target.value)}
      />
    </FormField>
  )
}

const writeText = {
  create: "clusterBackups.plan.create",
  patch: "clusterBackups.plan.patch",
  delete: "notify.plan.delete",
} as const

function DestinationsPlan({ planned, onClose }: { planned: Planned; onClose: () => void }) {
  const t = useT()
  const plan = useQuery(s3DestinationsPlanQuery(planned.request))
  const apply = useMutation({
    mutationFn: async (token: string) => await zeabur.applyS3Destinations({ token }),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onError: refresh,
  })
  return (
    <PlanFrame
      title={planned.title}
      plan={plan}
      apply={apply}
      action={planned.destructive ? t("projectSettings.removeAction") : t("s3d.save")}
      destructive={planned.destructive}
      ready={(data) => (data?.writes.length ?? 0) > 0}
      onClose={onClose}
    >
      {(data) => (
        <>
          {data.writes.length === 0 ? <p>{t("clusterBackups.plan.nothing")}</p> : null}
          <ul className="flex flex-col gap-1">
            {data.writes.map((w) => (
              <li key={`${w.namespace}/${w.kind}/${w.name}`}>
                {w.copy
                  ? t("s3d.plan.copy", w.name, w.namespace)
                  : t(writeText[w.action], `${w.kind}/${w.name}`)}
              </li>
            ))}
          </ul>
          <KeptCopies plan={data} />
          <p className="type-meta text-muted-foreground">{t("clusterBackups.plan.keysHidden")}</p>
          <PlanDiffs writes={data.writes} />
        </>
      )}
    </PlanFrame>
  )
}

/** The copies that keep the old keys, and why. */
function KeptCopies({ plan }: { plan: Schemas.S3DestinationsPlan }) {
  const t = useT()
  const names = (reason: Schemas.CopyKept) =>
    plan.keptCopies.filter((c) => c.reason === reason).map((c) => `${c.namespace}/${c.name}`)
  const denied = names("denied")
  const unknown = names("unknownKeys")
  const lines = [
    plan.copiesUnknown ? t("s3d.plan.unsearched") : undefined,
    denied.length > 0 ? t("s3d.plan.keptDenied", t("common.list", denied)) : undefined,
    unknown.length > 0 ? t("s3d.plan.keptUnknown", t("common.list", unknown)) : undefined,
  ].filter((line) => line !== undefined)
  if (lines.length === 0) return null
  return (
    <Alert variant="warn">
      <WarningIcon aria-hidden="true" />
      <AlertDescription>{t("common.sentences", lines)}</AlertDescription>
    </Alert>
  )
}
