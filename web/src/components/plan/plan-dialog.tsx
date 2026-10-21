import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useEffect, useState } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { api, toProblem, unwrap, type Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { actionOf, describe, sentenceOf } from "@/components/plan/describe"
import { carryOut } from "@/components/plan/outcome"
import type { PlanAsk } from "@/components/plan/planner"
import { PlanPreview } from "@/components/plan/preview"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { YamlDiff } from "@/components/yaml"
import { useI18n, useT, type Translate } from "@/i18n"
import { awaitsName, destinationOf, typedName, undoOf, weigh, type PodsOf } from "@/model/risk"

const COUNTDOWN_FROM_SECONDS = 120

export function PlanDialog({
  project,
  ask,
  onClose,
  onApplied,
  reopen,
  onRefused,
}: {
  project: string
  ask: PlanAsk
  onClose: () => void
  onApplied?: () => void
  reopen: (ask: PlanAsk) => void
  onRefused?: (fields: Schemas["FieldCause"][]) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const navigate = useNavigate()
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey: ["plan", project, ask],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.POST("/api/v1/projects/{project}/plan", {
          params: { path: { project } },
          body: { changes: ask.changes, restartAfter: ask.restartAfter },
          signal,
        }),
      ),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const document = useQuery({ ...projectQuery(project, { poll: false }), enabled: false })
  const podsOf: PodsOf = (workload) => {
    const service = document.data?.services.find(
      (s) => s.kind === workload.kind && s.name === workload.name,
    )
    return service?.replicas ?? undefined
  }
  const weight = weigh(ask.changes, podsOf)
  const light = weight.risk === "low"
  const left = useSecondsLeft(plan.data?.expiresAt)
  const expired = left !== undefined && left <= 0
  const description = plan.data ? describe(t, ask, plan.data, podsOf, weight.reasons) : undefined
  const named = typedName(ask.changes)
  const [typed, setTyped] = useState("")
  const unconfirmed = awaitsName(ask.changes, typed)

  const confirm = () => {
    if (!plan.data) return
    void carryOut({
      project,
      token: plan.data.token,
      ask,
      t,
      locale,
      view: destinationOf(project, ask.changes, plan.data, ask.service),
      undo: (applied) => {
        const changes = undoOf(ask.changes, podsOf, applied)
        return changes
          ? { changes, title: changes.map((c) => sentenceOf(t, c, () => undefined)).join(" ") }
          : undefined
      },
      navigate: (href) => void navigate({ href }),
      reopen,
      onApplied,
    })
    onClose()
  }
  const cancel = () => {
    if (plan.isError) onRefused?.(toProblem(plan.error).fields ?? [])
    onClose()
  }

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : cancel())}>
      <DialogContent variant={light ? "compact" : "wide"}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{ask.title}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {plan.isPending || plan.isFetching ? (
            <LoadingRows label={t("plan.planning")} rows={light ? 1 : 3} />
          ) : plan.isError ? (
            <Problem error={plan.error} />
          ) : description ? (
            <PlanBody
              plan={plan.data}
              lines={description.lines}
              consequence={weight.risk === "high" ? (description.consequence ?? "") : undefined}
            />
          ) : null}
          {named !== undefined && plan.data ? (
            <div className="mt-4 flex flex-col gap-2">
              <Label htmlFor="plan-typed-name">{t("plan.typeName", named)}</Label>
              <Input
                id="plan-typed-name"
                value={typed}
                onChange={(event) => setTyped(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                translate="no"
              />
            </div>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center gap-2 border-t px-5 py-3">
          {plan.data && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
            <span className="type-meta text-muted-foreground tabular-nums">
              {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
            </span>
          ) : null}
          <span className="ml-auto flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={cancel}>
              {t("plan.cancel")}
            </Button>
            {expired ? (
              <Button size="sm" onClick={() => void plan.refetch()}>
                {t("plan.replan")}
              </Button>
            ) : (
              <Button
                size="sm"
                variant={weight.risk === "high" ? "destructive" : "default"}
                disabled={!plan.data || plan.isFetching || unconfirmed}
                onClick={confirm}
              >
                {description?.action ?? actionOf(t, ask)}
              </Button>
            )}
          </span>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function useSecondsLeft(expiresAt?: string): number | undefined {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])
  return expiresAt ? Math.max(0, Math.round((Date.parse(expiresAt) - now) / 1000)) : undefined
}

function clock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`
}

/** Warnings the plain sentences already say. */
const said = new Set<Schemas["PlanWarning"]["code"]>([
  "restartNeeded",
  "restartAdded",
  "configNotRolledBack",
])

function PlanBody({
  plan,
  lines,
  consequence,
}: {
  plan: Schemas["PlanResponse"]
  lines: string[]
  consequence?: string
}) {
  const t = useT()
  const [first, ...rest] = lines
  const warnings = plan.warnings.filter((w) => !said.has(w.code))
  return (
    <div className="flex flex-col gap-3">
      {consequence !== undefined && first ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>
            {first}
            {consequence ? ` ${consequence}` : null}
          </AlertDescription>
        </Alert>
      ) : first ? (
        <p className="text-sm">{first}</p>
      ) : null}
      {rest.map((line) => (
        <p key={line} className="text-sm text-muted-foreground">
          {line}
        </p>
      ))}
      <PlanPreview
        preview={plan.preview}
        warnedClaims={plan.warnings.flatMap((w) =>
          w.code === "claimDataMayBeLost" ? [w.claim] : [],
        )}
      />
      {warnings.length > 0 ? (
        <ul className="flex flex-col gap-1.5" aria-label={t("plan.warnings")}>
          {warnings.map((warning) => (
            <li key={JSON.stringify(warning)} className="flex items-start gap-2 text-sm">
              <StatusDot health="warn" className="mt-1.5" />
              <span>{warningText(t, warning)}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <Collapsible>
        <CollapsibleTrigger variant="disclosure" className="mt-1">
          <CaretRightIcon aria-hidden="true" />
          {t("plan.details")}
        </CollapsibleTrigger>
        <CollapsibleContent className="-mx-1">
          <div className="flex flex-col gap-4 px-1 pt-3 pb-1">
            {plan.objects.map((object) => (
              <PlannedObject
                key={`${object.target.kind}/${object.target.name}`}
                object={object}
                named={plan.objects.length > 1}
              />
            ))}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

function PlannedObject({ object, named }: { object: Schemas["PlannedObject"]; named: boolean }) {
  const t = useT()
  const before = object.beforeYaml ?? ""
  const after = object.afterYaml ?? ""
  const owned = object.owners.filter((o) => o.managers.some((m) => m !== "zeabur"))
  return (
    <section
      aria-label={`${object.target.kind}/${object.target.name}`}
      className="flex flex-col gap-2"
    >
      {named ? (
        <header className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-sm font-medium" translate="no">
            {object.target.kind}/{object.target.name}
          </span>
          {object.before ? null : (
            <span className="type-meta text-muted-foreground">{t("plan.create")}</span>
          )}
          {object.write.type === "delete" ? (
            <span className="type-meta text-muted-foreground">{t("plan.delete")}</span>
          ) : null}
        </header>
      ) : null}
      {owned.length > 0 ? (
        <p className="type-meta text-muted-foreground">
          {t("plan.owners")}:{" "}
          <span className="font-mono" translate="no">
            {owned.map((o) => `${o.path} (${o.managers.join(", ")})`).join(t("common.sep"))}
          </span>
        </p>
      ) : null}
      <YamlDiff before={before} after={after} />
    </section>
  )
}

function names(workloads: Schemas["WorkloadRef"][]): string {
  return workloads.map((w) => w.name).join(", ")
}

function warningText(t: Translate, warning: Schemas["PlanWarning"]): string {
  switch (warning.code) {
    case "restartNeeded":
      return t("warn.restartNeeded", names(warning.workloads))
    case "restartAdded":
      return t("warn.restartAdded", names(warning.workloads))
    case "tlsHostChanged":
      return t("warn.tlsHostChanged", warning.hostname, warning.route)
    case "gatewayChosen":
      return t("warn.gatewayChosen", `${warning.namespace}/${warning.name}`, warning.of)
    case "gatewaysUnreadable":
      return t("warn.gatewaysUnreadable")
    case "secretValuesUnknown":
      return t("warn.secretValuesUnknown", warning.name)
    case "configNotRolledBack":
      return t("warn.configNotRolledBack", warning.workload.name)
    case "storageClassUnknown":
      return t("warn.storageClassUnknown", warning.claim)
    case "readWriteOnceShared":
      return t("warn.readWriteOnceShared", warning.workload.name)
    case "readWriteOncePodShared":
      return t("warn.readWriteOncePodShared", warning.workload.name)
    case "deletesDependents":
      return t(
        "warn.deletesDependents",
        warning.target.name,
        warning.dependents.map((d) => `${d.kind} ${d.name}`).join(", "),
      )
    case "podReplaced":
      return t("warn.podReplaced", warning.pod, `${warning.owner.kind} ${warning.owner.name}`)
    case "claimDataMayBeLost":
      return warning.reclaimPolicy === "Retain"
        ? t("warn.claimDataRetained", warning.claim, warning.volume ?? "")
        : warning.reclaimPolicy === "Delete"
          ? t("warn.claimDataDeleted", warning.claim, warning.volume ?? "")
          : t("warn.claimDataUnknown", warning.claim)
    case "stillUsed":
      return warning.target.kind === "PersistentVolumeClaim"
        ? t("warn.claimStillMounted", warning.target.name, names(warning.workloads))
        : t("warn.stillUsed", warning.target.name, names(warning.workloads))
    case "networkPolicyRemoved":
      return [
        warning.ingressOpened.length > 0
          ? t("warn.ingressOpened", names(warning.ingressOpened))
          : undefined,
        warning.egressOpened.length > 0
          ? t("warn.egressOpened", names(warning.egressOpened))
          : undefined,
        warning.connectionsOpened.length > 0
          ? t(
              "warn.connectionsOpened",
              warning.connectionsOpened.map((c) => `${c.from.name} → ${c.to.name}`).join(", "),
            )
          : undefined,
      ]
        .filter(Boolean)
        .join(" ")
  }
}
