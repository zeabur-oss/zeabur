import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import WarningIcon from "~icons/ph/warning"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"
import { PlanDiffs, PlanFrame } from "@/components/cluster-plan"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { s3DestinationsQuery } from "@/components/s3-fields"
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
import { FormButtons, Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"

type Request = Schemas.S3DestinationsPlanRequest
type Planned = { request: Request; title: string; destructive?: boolean }

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: s3DestinationsQuery.queryKey })
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
          gate={gate}
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
          {[where, destination.endpoint, destination.region].filter(Boolean).join(t("common.sep"))}
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
  key: keyof DestinationForm
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
  gate,
  onCancel,
  onReview,
}: {
  editing?: Destination
  taken: string[]
  gate: Gate
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
      value={form[key]}
      placeholder={placeholder}
      autoComplete="off"
      spellCheck={false}
      disabled={key === "name" && editing !== undefined}
      onChange={(event) => setForm((f) => ({ ...f, [key]: event.target.value }))}
    />
  )
  return (
    <form
      className="flex flex-col gap-4 rounded-xl border border-main bg-card p-5"
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problems.length === 0) onReview(saveRequest(form))
      }}
    >
      <FormField label={t("vars.name")} errors={errors("name", "taken")}>
        {input("name", "r2-backups")}
      </FormField>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
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
      </div>
      <FormButtons gate={gate} onCancel={onCancel} />
    </form>
  )
}

const writeText = {
  create: "clusterBackups.plan.create",
  patch: "clusterBackups.plan.patch",
  delete: "notify.plan.delete",
} as const

function DestinationsPlan({ planned, onClose }: { planned: Planned; onClose: () => void }) {
  const t = useT()
  const plan = useQuery({
    queryKey: ["s3-destinations-plan", planned.request],
    queryFn: async ({ signal }) => await zeabur.planS3Destinations(planned.request, { signal }),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
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
                {w.kind === "Secret" && w.namespace !== S3D_NAMESPACE
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

const S3D_NAMESPACE = "zeabur-s3-destinations"

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
