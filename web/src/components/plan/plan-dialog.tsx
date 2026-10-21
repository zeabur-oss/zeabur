import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useEffect, useState } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import ProhibitIcon from "~icons/ph/prohibit"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { api, toProblem, unwrap, type Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { actionOf, describe, undoTitle, type Description } from "@/components/plan/describe"
import { bounded, type MountsOf } from "@/components/plan/groups"
import { PlanGroups } from "@/components/plan/groups-view"
import { carryOut } from "@/components/plan/outcome"
import type { PlanAsk } from "@/components/plan/planner"
import { PlanPreview, useBlockers } from "@/components/plan/preview"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
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
import { cn } from "@/lib/utils"
import { toolNotes, type Skipped, type ToolNote } from "@/model/managed"
import { awaitsName, destinationOf, typedName, undoOf, weigh, type PodsOf } from "@/model/risk"

const COUNTDOWN_FROM_SECONDS = 120

/** A sentence, and the full names behind its "and N more". */
interface Said {
  text: string
  names?: string[]
}

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
  const serviceOf = (workload: Schemas["WorkloadRef"]) =>
    document.data?.services.find((s) => s.kind === workload.kind && s.name === workload.name)
  const podsOf: PodsOf = (workload) => serviceOf(workload)?.replicas ?? undefined
  const mountsOf: MountsOf = (workload) =>
    (serviceOf(workload)?.containers ?? []).flatMap((c) =>
      c.mounts.filter((m) => m.category === "persistent").map((m) => m.mountPath),
    )
  const weight = weigh(ask.changes, podsOf)
  const left = useSecondsLeft(plan.data?.expiresAt)
  const expired = left !== undefined && left <= 0
  const description = plan.data
    ? describe(t, ask, plan.data, podsOf, weight.reasons, mountsOf)
    : undefined
  const grouped = !!description?.facts
  const light = weight.risk === "low" && !grouped
  const blockers = useBlockers(
    plan.data?.preview.fit ?? { status: "notNeeded", pods: 0, cpuMillicores: 0, memoryBytes: 0 },
  )
  const blocked = blockers.length > 0
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
        return changes ? { changes, title: undoTitle(t, changes, ask.changes) } : undefined
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
  const ready = !plan.isPending && !plan.isFetching && plan.data && description

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
        {ready ? (
          <Conclusion
            description={description}
            blockers={blockers}
            high={weight.risk === "high"}
            grouped={grouped}
          />
        ) : null}
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {plan.isPending || plan.isFetching ? (
            <LoadingRows label={t("plan.planning")} rows={light ? 1 : 3} />
          ) : plan.isError ? (
            <Problem error={plan.error} />
          ) : description ? (
            <PlanBody
              plan={plan.data}
              skipped={ask.skipped}
              description={description}
              alerted={!grouped && !description.risk && weight.risk === "high"}
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
          {plan.data && blocked ? (
            <span id="plan-blocked" className="type-meta text-fail-strong">
              {t("plan.blockedReason")}
            </span>
          ) : plan.data && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
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
                disabled={!plan.data || plan.isFetching || unconfirmed || blocked}
                aria-describedby={blocked ? "plan-blocked" : undefined}
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

/** What stays in view above the scrolling body: what blocks the plan, what it puts at risk and,
 * for many changes, the summary and its cost. */
function Conclusion({
  description,
  blockers,
  high,
  grouped,
}: {
  description: Description
  blockers: string[]
  high: boolean
  grouped: boolean
}) {
  const t = useT()
  const risk = description.risk ?? (high && grouped ? description.consequence : undefined)
  if (blockers.length === 0 && !risk && !grouped) return null
  return (
    <div className="flex flex-col gap-3 border-b px-5 py-3">
      {blockers.length > 0 ? (
        <Alert variant="destructive" aria-label={t("preview.blocked")}>
          <ProhibitIcon aria-hidden="true" />
          <AlertTitle>{t("plan.blockedReason")}</AlertTitle>
          <AlertDescription>
            {blockers.map((line) => (
              <p key={line}>{line}</p>
            ))}
          </AlertDescription>
        </Alert>
      ) : null}
      {risk ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>
            <Line said={{ text: risk, names: description.more[risk] }} />
          </AlertDescription>
        </Alert>
      ) : null}
      {description.facts ? (
        <p className="text-sm">
          {description.facts.map((fact, i) => (
            <span
              key={fact.text}
              className={cn(
                fact.tone === "warn" && "text-warn-strong",
                fact.tone === "ok" && "text-ok-strong",
              )}
            >
              {i > 0 ? t("common.sep") : null}
              {fact.text}
            </span>
          ))}
        </p>
      ) : null}
      {grouped && description.cost ? (
        <p className="text-sm text-muted-foreground">{description.cost}</p>
      ) : null}
    </div>
  )
}

/** A sentence; when it names only some, Show all lists the rest. */
function Line({ said, className }: { said: Said; className?: string }) {
  const t = useT()
  if (!said.names) return <p className={className}>{said.text}</p>
  return (
    <div className={className}>
      <Collapsible>
        <p>{said.text}</p>
        <CollapsibleTrigger variant="disclosure" className="mt-1">
          <CaretRightIcon aria-hidden="true" />
          {t("plan.showAll", said.names.length)}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <p className="pt-1 font-mono text-xs break-words" translate="no">
            {said.names.join(t("common.sep"))}
          </p>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

/** Warnings the plain sentences already say. */
const sentenced = new Set<Schemas["PlanWarning"]["code"]>([
  "restartNeeded",
  "restartAdded",
  "configNotRolledBack",
  "debugContainerStays",
  "debugContainerReused",
  "toolsPod",
  "toolsPodReused",
])

function PlanBody({
  plan,
  skipped,
  description,
  alerted,
}: {
  plan: Schemas["PlanResponse"]
  skipped?: Skipped[]
  description: Description
  /** The first line goes in a warning callout with the consequence. */
  alerted: boolean
}) {
  const t = useT()
  const { lines, more } = description
  const [first, ...rest] = lines
  const warnings = [
    ...plan.warnings.filter((w) => !sentenced.has(w.code)).map((w) => warningText(t, w)),
    ...toolNotes(plan, skipped).map((note) => noteText(t, note)),
  ]
  const grouped = !!description.facts
  return (
    <div className="flex flex-col gap-3">
      {alerted && first ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>
            {first}
            {description.consequence ? ` ${description.consequence}` : null}
          </AlertDescription>
        </Alert>
      ) : first ? (
        <Line said={{ text: first, names: more[first] }} className="text-sm" />
      ) : null}
      {rest.map((line) => (
        <Line
          key={line}
          said={{ text: line, names: more[line] }}
          className="text-sm text-muted-foreground"
        />
      ))}
      {!grouped && description.cost ? (
        <p className="text-sm text-muted-foreground">{description.cost}</p>
      ) : null}
      <PlanPreview
        preview={plan.preview}
        warnedClaims={plan.warnings.flatMap((w) =>
          w.code === "claimDataMayBeLost" ? [w.claim] : [],
        )}
        said={description.said}
        rollouts={!description.outcomes}
        removals={!description.edits}
      />
      {warnings.length > 0 ? (
        <ul className="flex flex-col gap-1.5" aria-label={t("plan.warnings")}>
          {warnings.map((warning) => (
            <li key={warning.text} className="flex items-start gap-2 text-sm">
              <StatusDot health="warn" className="mt-1.5" />
              <Line said={warning} className="min-w-0" />
            </li>
          ))}
        </ul>
      ) : null}
      {grouped ? <PlanGroups edits={description.edits} outcomes={description.outcomes} /> : null}
      <Collapsible>
        <CollapsibleTrigger variant="disclosure" className="mt-1">
          <CaretRightIcon aria-hidden="true" />
          {t("plan.details")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="flex flex-col gap-4 pt-3">
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

/** A sentence over a bounded list of names. */
function namedText(t: Translate, names: string[], sentence: (list: string) => string): Said {
  const { text, rest } = bounded(t, names)
  return { text: sentence(text), names: rest.length > 0 ? names : undefined }
}

function noteText(t: Translate, note: ToolNote): Said {
  const when = t("plan.when", note.tool)
  return note.kind === "skipped"
    ? namedText(t, note.names, (names) =>
        t("plan.toolSkipped", names, note.names.length, note.tool, when),
      )
    : { text: t("plan.toolPutBack", when, note.paths.join(", "), note.object) }
}

function names(t: Translate, workloads: Schemas["WorkloadRef"][]): string {
  return bounded(
    t,
    workloads.map((w) => w.name),
  ).text
}

function warningText(t: Translate, warning: Schemas["PlanWarning"]): Said {
  switch (warning.code) {
    case "deletesDependents":
      return namedText(
        t,
        warning.dependents.map((d) => `${d.kind} ${d.name}`),
        (list) => t("warn.deletesDependents", warning.target.name, list),
      )
    case "rightsMissing":
      return namedText(
        t,
        warning.rights.map((r) => `${r.verb} ${r.group ? `${r.resource}.${r.group}` : r.resource}`),
        (list) => t("warn.rightsMissing", list),
      )
    case "restartNeeded":
    case "restartAdded":
    case "stillUsed": {
      const all = warning.workloads.map((w) => w.name)
      const cut = bounded(t, all).rest.length > 0
      return { text: warningSentence(t, warning), names: cut ? all : undefined }
    }
    default:
      return { text: warningSentence(t, warning) }
  }
}

function warningSentence(
  t: Translate,
  warning: Exclude<Schemas["PlanWarning"], { code: "deletesDependents" | "rightsMissing" }>,
): string {
  switch (warning.code) {
    case "restartNeeded":
      return t("warn.restartNeeded", names(t, warning.workloads))
    case "restartAdded":
      return t("warn.restartAdded", names(t, warning.workloads))
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
        ? t("warn.claimStillMounted", warning.target.name, names(t, warning.workloads))
        : t("warn.stillUsed", warning.target.name, names(t, warning.workloads))
    case "networkPolicyRemoved":
      return [
        warning.ingressOpened.length > 0
          ? t("warn.ingressOpened", names(t, warning.ingressOpened))
          : undefined,
        warning.egressOpened.length > 0
          ? t("warn.egressOpened", names(t, warning.egressOpened))
          : undefined,
        warning.connectionsOpened.length > 0
          ? t(
              "warn.connectionsOpened",
              bounded(
                t,
                warning.connectionsOpened.map((c) => `${c.from.name} → ${c.to.name}`),
              ).text,
            )
          : undefined,
      ]
        .filter(Boolean)
        .join(" ")
    case "toolsCredentialExposed":
      return t("warn.toolsCredentialExposed", warning.namespace)
    case "debugContainerStays":
    case "debugContainerReused":
    case "toolsPod":
    case "toolsPodReused":
      return ""
  }
}
