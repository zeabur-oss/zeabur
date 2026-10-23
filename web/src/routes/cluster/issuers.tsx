// Cluster → Overview → Certificates: cert-manager's issuers, and a Let's Encrypt issuer with a
// DNS-01 solver, which is what signs wildcard domains.
import { useMutation, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState } from "react"
import PlusIcon from "~icons/ph/plus"

import type { Schemas } from "@/api/client"
import {
  applyClusterIssuer,
  clusterIssuersQuery,
  planClusterIssuer,
  queryClient,
} from "@/api/queries"
import { PlanDiffs, PlanFrame } from "@/components/cluster-plan"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import {
  PROVIDER_ORDER,
  PROVIDERS,
  issuerForm,
  issuerProblems,
  issuerRequest,
  solverLabel,
  type IssuerForm,
  type IssuerProblem,
} from "@/model/cluster-issuers"
import type { Gate } from "@/model/rights"
import { Section } from "@/routes/cluster/section"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"
import { Textarea } from "@zeabur/design-system/components/textarea"

type Data = Schemas["ClusterIssuers"]
type Issuer = Schemas["IssuerView"]

const readyHealth = { ready: "ok", notReady: "fail", unknown: "idle" } as const

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: clusterIssuersQuery.queryKey })
}

export function IssuersSection() {
  const t = useT()
  const issuers = useQuery(clusterIssuersQuery)
  const data = issuers.data
  const gate = useHintGate("changeIssuers", data?.canChange)
  const [editing, setEditing] = useState<{ issuer?: Issuer }>()
  const [request, setRequest] = useState<Schemas["IssuerRequest"]>()
  if (!data || data.state === "refused") return null
  const installed = data.state === "installed"
  return (
    <Section
      id="cluster-certificates"
      title={t("issuers.title")}
      count={installed ? data.issuers.length : undefined}
      action={
        installed && !editing ? (
          <GateTip gate={gate}>
            <Button size="sm" variant="outline" onClick={() => setEditing({})}>
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("issuers.add")}
            </Button>
          </GateTip>
        ) : null
      }
    >
      {installed ? (
        <Installed
          data={data}
          gate={gate}
          editing={editing}
          onEdit={setEditing}
          onReview={(next) => {
            setEditing(undefined)
            setRequest(next)
          }}
        />
      ) : (
        <p className="type-meta text-muted-foreground">
          {t("issuers.notInstalled")}{" "}
          <Link to="/cluster/components" className={buttonVariants({ variant: "link" })}>
            {t("issuers.install")}
          </Link>
        </p>
      )}
      {request ? <PlanDialog request={request} onClose={() => setRequest(undefined)} /> : null}
    </Section>
  )
}

function Installed({
  data,
  gate,
  editing,
  onEdit,
  onReview,
}: {
  data: Data
  gate: Gate
  editing?: { issuer?: Issuer }
  onEdit: (editing?: { issuer?: Issuer }) => void
  onReview: (request: Schemas["IssuerRequest"]) => void
}) {
  const t = useT()
  return (
    <>
      <Verdict data={data} />
      {data.issuers.length > 0 ? (
        <List
          data={data}
          gate={gate}
          onEdit={editing ? undefined : (issuer) => onEdit({ issuer })}
        />
      ) : null}
      {editing ? (
        <Editor
          key={editing.issuer?.name ?? ""}
          issuer={editing.issuer}
          taken={data.issuers.map((i) => i.name)}
          gate={gate}
          onCancel={() => onEdit(undefined)}
          onReview={onReview}
        />
      ) : null}
      <KubeDetails
        facts={[
          [t("issuers.kube.issuer"), "ClusterIssuer (cert-manager.io/v1)"],
          [t("issuers.kube.secret"), t("issuers.kube.secretValue")],
          [t("issuers.kube.domain"), "Ingress cert-manager.io/cluster-issuer"],
        ]}
      />
    </>
  )
}

function Verdict({ data }: { data: Data }) {
  const t = useT()
  const wildcard = data.issuers.find((i) => i.wildcards && i.ready === "ready")
  return (
    <p className="type-body text-secondary">
      {wildcard ? t("issuers.wildcardsBy", wildcard.name) : t("issuers.noWildcards")}
    </p>
  )
}

function List({
  data,
  gate,
  onEdit,
}: {
  data: Data
  gate: Gate
  onEdit?: (issuer: Issuer) => void
}) {
  const t = useT()
  return (
    <ul className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
      {data.issuers.map((issuer) => (
        <li key={issuer.name} className="flex min-w-0 flex-col gap-1 px-4 py-2.5">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <span className="min-w-0 truncate type-mono font-medium" translate="no">
              {issuer.name}
            </span>
            <StatusPill health={readyHealth[issuer.ready]} size="sm">
              {t(`issuers.ready.${issuer.ready}`)}
            </StatusPill>
            {issuer.name === data.default ? (
              <Badge variant="outline">{t("issuers.default")}</Badge>
            ) : null}
            {issuer.staging ? <Badge variant="warn">{t("issuers.staging")}</Badge> : null}
            <span className="flex-1" />
            {issuer.dns && onEdit ? (
              <GateTip gate={gate}>
                <Button size="xs" variant="outline" onClick={() => onEdit(issuer)}>
                  {t("issuers.edit")}
                </Button>
              </GateTip>
            ) : null}
          </div>
          <p className="type-meta text-muted-foreground">
            <span translate="no">{issuer.solvers.map(solverLabel).join(", ") || "—"}</span>
            {t("common.sep")}
            {issuer.wildcards ? t("issuers.wildcards") : t("issuers.noWildcard")}
          </p>
          {issuer.ready === "notReady" && issuer.message ? (
            <p className="type-meta break-words text-muted-foreground">{issuer.message}</p>
          ) : null}
        </li>
      ))}
    </ul>
  )
}

const formBox = "flex flex-col gap-4 rounded-xl border border-main bg-card p-5"

function Editor({
  issuer,
  taken,
  gate,
  onCancel,
  onReview,
}: {
  issuer?: Issuer
  taken: string[]
  gate: Gate
  onCancel: () => void
  onReview: (request: Schemas["IssuerRequest"]) => void
}) {
  const t = useT()
  const stagingId = useId()
  const [form, setForm] = useState<IssuerForm>(() => issuerForm(issuer))
  const [tried, setTried] = useState(false)
  const editing = issuer !== undefined
  const problems = issuerProblems(form, editing)
  const clash = !editing && taken.includes(form.name)
  const set = (patch: Partial<IssuerForm>) => setForm((v) => ({ ...v, ...patch }))
  const setValue = (key: string, value: string) =>
    setForm((v) => ({ ...v, values: { ...v.values, [key]: value } }))
  const error = (problem: IssuerProblem) =>
    tried && problems.includes(problem) ? [t(`issuers.problem.${problem}`)] : []
  const { fields, optional = [], secret } = PROVIDERS[form.provider]
  return (
    <form
      className={formBox}
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problems.length === 0 && !clash) onReview(issuerRequest(form))
      }}
    >
      <p className="type-meta text-muted-foreground">{t("issuers.lead")}</p>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("issuers.name")}
          errors={[...error("name"), ...(tried && clash ? [t("issuers.problem.taken")] : [])]}
        >
          <Input
            size="sm"
            mono
            value={form.name}
            disabled={editing}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => set({ name: event.target.value })}
          />
        </FormField>
        <FormField label={t("issuers.email")} hint={t("issuers.emailHint")} errors={error("email")}>
          <Input
            size="sm"
            type="email"
            value={form.email}
            autoComplete="off"
            onChange={(event) => set({ email: event.target.value })}
          />
        </FormField>
        <FormField label={t("issuers.provider")}>
          <Select
            size="sm"
            className="w-full"
            value={form.provider}
            onValueChange={(value) => set({ provider: value as IssuerForm["provider"] })}
            items={PROVIDER_ORDER.map((p) => ({ value: p, label: t(`issuers.providers.${p}`) }))}
          />
        </FormField>
        {fields.map((key) => (
          <FormField
            key={`${form.provider}-${key}`}
            label={t(`issuers.field.${key as "region"}`)}
            hint={optional.includes(key) ? t("issuers.optional") : undefined}
            errors={!optional.includes(key) && !form.values[key]?.trim() ? error("field") : []}
          >
            <Input
              size="sm"
              mono
              value={form.values[key] ?? ""}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setValue(key, event.target.value)}
            />
          </FormField>
        ))}
        <FormField
          className={form.provider === "cloudDns" ? "sm:col-span-2" : undefined}
          label={t(`issuers.field.${secret as "apiToken"}`)}
          hint={editing ? t("issuers.keepSecret") : t(`issuers.secretHint.${form.provider}`)}
          errors={error("secret")}
        >
          {form.provider === "cloudDns" ? (
            <Textarea
              mono
              rows={4}
              value={form.values[secret] ?? ""}
              spellCheck={false}
              onChange={(event) => setValue(secret, event.target.value)}
            />
          ) : (
            <Input
              size="sm"
              mono
              type="password"
              value={form.values[secret] ?? ""}
              autoComplete="new-password"
              onChange={(event) => setValue(secret, event.target.value)}
            />
          )}
        </FormField>
      </div>
      <div className="flex items-center gap-2">
        <Checkbox
          id={stagingId}
          checked={form.staging}
          onCheckedChange={(staging) => set({ staging: staging === true })}
        />
        <Label htmlFor={stagingId}>{t("issuers.useStaging")}</Label>
      </div>
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
          {t("edit.cancel")}
        </Button>
        <GateTip gate={gate}>
          <Button size="sm" type="submit">
            {t("edit.review")}
          </Button>
        </GateTip>
      </div>
    </form>
  )
}

function PlanDialog({
  request,
  onClose,
}: {
  request: Schemas["IssuerRequest"]
  onClose: () => void
}) {
  const t = useT()
  const plan = useQuery({
    queryKey: ["cluster-issuer-plan", request],
    queryFn: ({ signal }) => planClusterIssuer(request, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyClusterIssuer(token),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onError: refresh,
  })
  return (
    <PlanFrame
      title={t("issuers.planTitle", request.name)}
      plan={plan}
      apply={apply}
      action={t("plan.apply")}
      ready={(data) => (data?.writes.length ?? 0) > 0}
      onClose={onClose}
    >
      {(data) =>
        data.writes.length === 0 ? (
          <p>{t("issuers.nothing")}</p>
        ) : (
          <>
            <ul className="flex flex-col gap-1">
              {data.writes.map((w) => (
                <li key={`${w.kind}/${w.name}`}>
                  {t(
                    w.action === "create" ? "issuers.plan.create" : "issuers.plan.patch",
                    `${w.kind}/${w.name}`,
                  )}
                </li>
              ))}
            </ul>
            {data.writes.some((w) => w.kind === "Secret") ? (
              <p className="type-meta text-muted-foreground">{t("issuers.secretHidden")}</p>
            ) : null}
            <PlanDiffs writes={data.writes} />
          </>
        )
      }
    </PlanFrame>
  )
}
