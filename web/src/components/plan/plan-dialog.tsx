import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useRef, useState, type ReactNode } from "react"
import { createPortal } from "react-dom"
import ArrowsClockwiseIcon from "~icons/ph/arrows-clockwise"
import CaretLeftIcon from "~icons/ph/caret-left"
import CaretRightIcon from "~icons/ph/caret-right"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import {
  actionOf,
  claimDataSaid,
  describe,
  restartRuleApplies,
  undoTitle,
  type Description,
} from "@/components/plan/describe"
import { bounded, type MountsOf } from "@/components/plan/groups"
import { PlanGroups } from "@/components/plan/groups-view"
import { COUNTDOWN_FROM_SECONDS, clock, useSecondsLeft } from "@/components/plan/expiry"
import { carryOut } from "@/components/plan/outcome"
import type { PlanAsk, PlanHooks } from "@/components/plan/planner"
import { PlanPreview, useShortfall } from "@/components/plan/preview"
import { ProjectDeletion } from "@/components/plan/project-deletion"
import { changedSince, type Reviewed } from "@/components/plan/replan"
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
import { useI18n, useT, writeSentence, type Translate } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { cn } from "@/lib/utils"
import { toolNotes, type Skipped, type ToolNote } from "@/model/managed"
import {
  aroundName,
  awaitsName,
  destinationOf,
  typedName,
  undoOf,
  weigh,
  type PodsOf,
} from "@/model/risk"
import type { Health } from "@/model/service"

/** A sentence, and the full names behind its "and N more". */
interface Said {
  text: string
  names?: string[]
  /** A plan line's dot; warn when left out. */
  health?: Health
}

interface PlanProps {
  project: string
  ask: PlanAsk
  /** The plan an apply refused because the project moved on since: this one is made afresh. */
  reviewed?: Reviewed
  onClose: () => void
  /** After the apply starts; `onClose` when left out. */
  onConfirmed?: () => void
  onApplied?: () => void
  reopen: (ask: PlanAsk, reviewed?: Reviewed) => void
  onRefused?: (problem: ProblemBody) => void
  hooks?: PlanHooks
}

/** A form's summary already says what its edits do; they are not hand-written YAML. */
const weighAsk = (ask: PlanAsk, podsOf: PodsOf) =>
  weigh(ask.summary ? ask.changes.filter((c) => c.op !== "edit") : ask.changes, podsOf)

/** The plan of an ask: what it says, and its buttons. */
function usePlan(
  { project, ask, reviewed, onClose, onConfirmed, onApplied, reopen, onRefused, hooks }: PlanProps,
  bare = false,
) {
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
          body: { changes: ask.changes },
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
  const weight = weighAsk(ask, podsOf)
  const left = useSecondsLeft(plan.data?.expiresAt)
  const expired = left !== undefined && left <= 0
  const description = plan.data
    ? describe(t, ask, plan.data, podsOf, weight.reasons, mountsOf)
    : undefined
  const grouped = !!description?.facts
  const light = weight.risk === "low" && !grouped
  const short = useShortfall(plan.data?.preview)
  const named = typedName(ask.changes, project)
  const [typed, setTyped] = useState("")
  const typedInput = useRef<HTMLInputElement>(null)
  const unconfirmed = awaitsName(ask.changes, typed, project)

  const confirm = () => {
    if (!plan.data) return
    void carryOut({
      project,
      token: plan.data.token,
      ask,
      t,
      locale,
      view: ask.view ?? destinationOf(project, ask.changes, plan.data, ask.service),
      undo: (applied) => {
        const changes = undoOf(ask.changes, podsOf, applied)
        return changes ? { changes, title: undoTitle(t, changes, ask.changes) } : undefined
      },
      navigate: (href) => void navigate({ href }),
      reopen,
      reviewed: plan.data,
      onApplied,
      onPartial: hooks?.onPartial,
    })
    ;(onConfirmed ?? onClose)()
  }
  const cancel = () => {
    if (plan.isError) onRefused?.(toProblem(plan.error))
    onClose()
  }
  const refusedAt = plan.isError ? toProblem(plan.error).at : undefined
  const where = refusedAt && hooks?.where ? hooks.where(refusedAt) : undefined
  const ready = !plan.isPending && !plan.isFetching && plan.data && description

  const conclusion = ready ? (
    <Conclusion
      replanned={reviewed ? <Replanned reviewed={reviewed} plan={plan.data} /> : undefined}
      description={description}
      short={short}
      high={weight.risk === "high"}
      grouped={grouped}
      bare={bare}
    />
  ) : null
  const content = (
    <>
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={light ? 1 : 3} />
      ) : plan.isError ? (
        <div className="flex flex-col gap-3">
          {where ? <p className="type-meta text-muted-foreground">{where}</p> : null}
          <Problem error={plan.error} />
          {where ? (
            <div>
              <Button variant="outline" size="xs" onClick={cancel}>
                {t("create.showInYaml")}
              </Button>
            </div>
          ) : null}
        </div>
      ) : description ? (
        <PlanBody
          plan={plan.data}
          skipped={ask.skipped}
          notes={ask.notes}
          description={description}
          alerted={!grouped && !description.risk && weight.risk === "high"}
        />
      ) : null}
      {plan.data && !plan.isFetching ? (
        <>
          <Readers
            project={project}
            ask={ask}
            plan={plan.data}
            reopen={reopen}
            go={(href) => {
              onClose()
              void navigate({ href })
            }}
          />
          <ProjectDeletion project={project} plan={plan.data} reopen={reopen} />
        </>
      ) : null}
      {named !== undefined && plan.data ? (
        <div className="mt-4 flex flex-col gap-2">
          <TypeNameLabel
            name={named}
            onFill={() => {
              setTyped(named)
              typedInput.current?.focus()
            }}
          />
          <Input
            ref={typedInput}
            id="plan-typed-name"
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            translate="no"
          />
        </div>
      ) : null}
    </>
  )
  const countdown =
    plan.data && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
      <span className="type-meta text-muted-foreground tabular-nums">
        {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
      </span>
    ) : null
  const action = expired ? (
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
  )
  return { light, cancel, conclusion, content, countdown, action }
}

export function PlanDialog(props: PlanProps) {
  const t = useT()
  const { light, cancel, conclusion, content, countdown, action } = usePlan(props)
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : cancel())}>
      <DialogContent variant={light ? "compact" : "wide"}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{props.ask.title}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        {conclusion}
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{content}</div>
        <div className="flex flex-wrap items-center gap-2 border-t px-5 py-3">
          {countdown}
          <span className="ml-auto flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={cancel}>
              {t("plan.cancel")}
            </Button>
            {action}
          </span>
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** The plan as the second step of a drawer: its body here, its buttons in the drawer's footer.
 * Back returns to the form. */
export function PlanStep({ footer, ...props }: PlanProps & { footer: HTMLElement | null }) {
  const t = useT()
  const { cancel, conclusion, content, countdown, action } = usePlan(props, true)
  return (
    <div className="flex flex-col gap-4">
      {conclusion}
      {content}
      {footer
        ? createPortal(
            <div className="flex w-full flex-wrap items-center gap-2">
              <Button variant="outline" size="sm" onClick={cancel}>
                <CaretLeftIcon aria-hidden="true" data-icon="inline-start" />
                {t("plan.back")}
              </Button>
              {countdown}
              <span className="ml-auto">{action}</span>
            </div>,
            footer,
          )
        : null}
    </div>
  )
}

/** What stays in view above the scrolling body: new pods that do not fit, what the plan puts at
 * risk and, for many changes, the summary and its cost. */
function Conclusion({
  replanned,
  description,
  short,
  high,
  grouped,
  bare,
}: {
  replanned?: ReactNode
  description: Description
  short?: ReturnType<typeof useShortfall>
  high: boolean
  grouped: boolean
  /** Inside a drawer's body, without the dialog's band. */
  bare?: boolean
}) {
  const t = useT()
  const risk = description.risk ?? (high && grouped ? description.consequence : undefined)
  if (!replanned && !short && !risk && !grouped) return null
  return (
    <div className={cn("flex flex-col gap-3", !bare && "border-b px-5 py-3")}>
      {replanned}
      {short ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertTitle>{t("plan.short.title")}</AlertTitle>
          <AlertDescription>
            <p>{short.cost}</p>
            {short.reasons.map((line) => (
              <p key={line}>{line}</p>
            ))}
            <p>{short.fix}</p>
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

/** The plan made again after an apply was refused because the project moved on: what it writes
 * now that it did not, and what it no longer writes. */
function Replanned({ reviewed, plan }: { reviewed: Reviewed; plan: Reviewed }) {
  const t = useT()
  const { added, dropped } = changedSince(reviewed, plan)
  return (
    <Alert variant="info" role="status">
      <ArrowsClockwiseIcon aria-hidden="true" />
      <AlertDescription>
        <p>{t("plan.replanned")}</p>
        {added.length > 0 ? (
          <Line said={namedText(t, added, (list) => t("plan.replannedAdded", list))} />
        ) : null}
        {dropped.length > 0 ? (
          <Line said={namedText(t, dropped, (list) => t("plan.replannedDropped", list))} />
        ) : null}
      </AlertDescription>
    </Alert>
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

/** The services a data change leaves on old values: restart them in the same plan, or, for one,
 * open its setting that restarts it on the next change. */
function Readers({
  project,
  ask,
  plan,
  reopen,
  go,
}: {
  project: string
  ask: PlanAsk
  plan: Schemas["PlanResponse"]
  reopen: (ask: PlanAsk) => void
  go: (href: string) => void
}) {
  const t = useT()
  const waiting = plan.warnings.flatMap((w) => (w.code === "restartNeeded" ? w.workloads : []))
  const [one] = waiting
  if (!one || ask.changes.some((c) => c.op === "restart")) return null
  const restarts = waiting.map((workload): Schemas["Change"] => ({ op: "restart", workload }))
  return (
    <div className="mt-3 flex flex-wrap items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        onClick={() => reopen({ ...ask, changes: [...ask.changes, ...restarts] })}
      >
        {t("plan.alsoRestart", waiting.length)}
      </Button>
      {waiting.length === 1 ? (
        <Button
          variant="ghost"
          size="sm"
          onClick={() =>
            go(
              `/projects/${encodeURIComponent(project)}/services/${one.kind}/${encodeURIComponent(one.name)}?tab=settings#compute`,
            )
          }
        >
          {t("plan.restartByItself")}
        </Button>
      ) : null}
    </div>
  )
}

/** Warnings the plain sentences already say. */
const sentenced = new Set<Schemas["PlanWarning"]["code"]>([
  "restartNeeded",
  "reloaderRestarts",
  "configNotRolledBack",
  "debugContainerStays",
  "debugContainerReused",
  "toolsPod",
  "toolsPodReused",
])

/** A database open to the internet is worse than the other high risks. */
function alertVariant(warnings: Schemas["PlanWarning"][]): "destructive" | "warn" {
  return warnings.some((w) => w.code === "publicPort" && w.dataStore) ? "destructive" : "warn"
}

function PlanBody({
  plan,
  skipped,
  notes = [],
  description,
  alerted,
}: {
  plan: Schemas["PlanResponse"]
  skipped?: Skipped[]
  notes?: string[]
  description: Description
  /** The first line goes in a warning callout with the consequence. */
  alerted: boolean
}) {
  const t = useT()
  const { lines, more } = description
  const [first, ...rest] = lines
  const warnings: Said[] = [
    ...warningsSaid(t, plan.warnings),
    ...(plan.gaps ?? []).flatMap((gap) => {
      const text = writeSentence(t, gap.message)
      return text ? [{ text }] : []
    }),
    ...toolNotes(plan, skipped).map((note) => noteText(t, note)),
  ]
  const grouped = !!description.facts
  return (
    <div className="flex flex-col gap-3">
      {alerted && first ? (
        <Alert variant={alertVariant(plan.warnings)} role="note">
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
      {notes.map((note) => (
        <p key={note} className="text-sm text-muted-foreground">
          {note}
        </p>
      ))}
      {!grouped && description.cost ? (
        <p className="text-sm text-muted-foreground">{description.cost}</p>
      ) : null}
      {restartRuleApplies(plan) ? (
        <p className="text-sm text-muted-foreground">{t("restart.rule")}</p>
      ) : null}
      <PlanPreview
        preview={plan.preview}
        warnedClaims={plan.warnings.flatMap((w) =>
          w.code === "claimDataMayBeLost" ? [w.claim] : [],
        )}
        said={description.said}
        rollouts={!description.outcomes}
        removals={!description.edits && !description.removalsSaid}
      />
      {warnings.length > 0 ? (
        <ul className="flex flex-col gap-1.5" aria-label={t("plan.warnings")}>
          {warnings.map((warning) => (
            <li key={warning.text} className="flex items-start gap-2 text-sm">
              <StatusDot health={warning.health ?? "warn"} className="mt-1.5" />
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

/** "Type <name> to confirm", where the name is a button that types it. */
function TypeNameLabel({ name, onFill }: { name: string; onFill: () => void }) {
  const t = useT()
  const [before, after] = aroundName((n) => t("plan.typeName", n))
  return (
    <Label htmlFor="plan-typed-name" className="block">
      {before}
      <button
        type="button"
        translate="no"
        title={t("plan.fillName")}
        className="cursor-pointer font-mono text-foreground underline decoration-dotted underline-offset-2 hover:decoration-solid"
        onClick={onFill}
      >
        {name}
      </button>
      {after}
    </Label>
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

/** The warnings the plain sentences do not say; a project's deletion has its own section. */
function warningsSaid(t: Translate, warnings: Schemas["PlanWarning"][]): Said[] {
  return warnings.flatMap((w): Said[] => {
    if (sentenced.has(w.code) || w.code === "deletesProject") return []
    if (w.code === "recreated")
      return [
        namedText(
          t,
          w.objects.map((o) => `${kindName(t, o.kind)} ${o.name}`),
          (list) => t("warn.recreated", list, w.tool),
        ),
      ]
    return [warningText(t, w)]
  })
}

function warningText(
  t: Translate,
  warning: Exclude<Schemas["PlanWarning"], { code: "recreated" | "deletesProject" }>,
): Said {
  switch (warning.code) {
    case "deletesDependents":
      return namedText(
        t,
        warning.dependents.map((d) => `${kindName(t, d.kind)} ${d.name}`),
        (list) => t("warn.deletesDependents", warning.target.name, list),
      )
    case "deletesOwned":
      return namedText(
        t,
        warning.objects.map((o) => `${kindName(t, o.kind)} ${o.name}`),
        (list) => t("warn.deletesOwned", warning.target.name, list),
      )
    case "stays":
      return { text: staysSentence(t, warning), health: "idle" }
    case "claimDataMayBeLost": {
      const said = claimDataSaid(t, warning)
      return { text: said.text, health: said.kept ? "warn" : "fail" }
    }
    case "rightsMissing":
      return namedText(
        t,
        warning.rights.map((r) => `${r.verb} ${r.group ? `${r.resource}.${r.group}` : r.resource}`),
        (list) => t("warn.rightsMissing", list),
      )
    case "restartNeeded":
    case "reloaderRestarts":
    case "stillUsed": {
      const all = warning.workloads.map((w) => w.name)
      const cut = bounded(t, all).rest.length > 0
      return { text: warningSentence(t, warning), names: cut ? all : undefined }
    }
    default:
      return { text: warningSentence(t, warning), health: healthOf(warning) }
  }
}

function healthOf(warning: Schemas["PlanWarning"]): Said["health"] {
  switch (warning.code) {
    case "httpsCertificate":
      return "ok"
    case "taskImageCopied":
      return "idle"
    case "publicPort":
      return warning.dataStore ? "fail" : "warn"
    case "publicPortBlocked":
      return "fail"
    default:
      return undefined
  }
}

function staysSentence(
  t: Translate,
  warning: Extract<Schemas["PlanWarning"], { code: "stays" }>,
): string {
  const object = `${kindName(t, warning.object.kind)} ${warning.object.name}`
  switch (warning.reason) {
    case "shared":
      return t("warn.staysShared", object, names(t, warning.workloads ?? []))
    case "routesElsewhere":
      return t("warn.staysRoute", object)
    case "controlled":
      return t(
        "warn.staysControlled",
        object,
        warning.owner ? `${kindName(t, warning.owner.kind)} ${warning.owner.name}` : "",
      )
    case "managed":
      return t("warn.staysManaged", object, warning.tool ?? "")
    case "notMade":
      return t("warn.staysNotMade", object)
  }
}

function warningSentence(
  t: Translate,
  warning: Exclude<
    Schemas["PlanWarning"],
    {
      code:
        | "deletesDependents"
        | "deletesOwned"
        | "stays"
        | "rightsMissing"
        | "claimDataMayBeLost"
        | "recreated"
        | "deletesProject"
    }
  >,
): string {
  switch (warning.code) {
    case "restartNeeded":
      return t("warn.restartNeeded", names(t, warning.workloads))
    case "reloaderRestarts":
      return warning.confirmed
        ? t("warn.reloaderRestarts", names(t, warning.workloads))
        : t("warn.reloaderMayRestart", names(t, warning.workloads))
    case "tlsHostChanged":
      return t("warn.tlsHostChanged", warning.hostname, warning.route)
    case "httpsCertificate":
      return t("warn.httpsCertificate", warning.hostname, warning.issuer)
    case "httpOnly":
      return httpOnlySentence(t, warning)
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
      return t(
        "warn.podReplaced",
        warning.pod,
        `${kindName(t, warning.owner.kind)} ${warning.owner.name}`,
      )
    case "stillUsed":
      return warning.target.kind === "PersistentVolumeClaim"
        ? t("warn.claimStillMounted", warning.target.name, names(t, warning.workloads))
        : t("warn.stillUsed", warning.target.name, names(t, warning.workloads))
    case "networkPolicyRemoved":
      return t("common.sentences", [
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
      ])
    case "debugContainerStays":
    case "debugContainerReused":
    case "toolsPod":
    case "toolsPodReused":
      return ""
    default:
      return accessWarning(t, warning)
  }
}

function httpOnlySentence(
  t: Translate,
  warning: Extract<Schemas["PlanWarning"], { code: "httpOnly" }>,
): string {
  switch (warning.reason) {
    case "certificatesElsewhere":
      return t("warn.httpOnlyElsewhere", warning.hostname, warning.route ?? "")
    case "wildcardNeedsDns01":
      return t("warn.httpOnlyWildcard", warning.hostname)
    default:
      return t("warn.httpOnlyNoIssuer", warning.hostname)
  }
}

function routeWarning(
  t: Translate,
  warning: Extract<
    Schemas["PlanWarning"],
    {
      code: "ownCertificate" | "issuerShared" | "routeDeleted" | "domainMoved"
    }
  >,
): string {
  switch (warning.code) {
    case "ownCertificate":
      return t("warn.ownCertificate", warning.hostname, warning.notAfter.slice(0, 10))
    case "issuerShared":
      return t(
        "warn.issuerShared",
        warning.route,
        warning.issuer,
        t("common.list", warning.hostnames),
      )
    case "domainMoved":
      return t(
        "warn.domainMoved",
        warning.hostname,
        `${kindName(t, warning.from.kind)} ${warning.from.name}`,
        `${kindName(t, warning.to.kind)} ${warning.to.name}`,
      )
    case "routeDeleted":
      return t(
        "warn.routeDeleted",
        `${kindName(t, warning.route.kind)} ${warning.route.name}`,
        warning.hostname,
      )
  }
}

function accessWarning(
  t: Translate,
  warning: Extract<
    Schemas["PlanWarning"],
    {
      code:
        | "taskImageCopied"
        | "permanentRedirect"
        | "passwordOverHttp"
        | "publicPort"
        | "publicPortBlocked"
        | "ownCertificate"
        | "issuerShared"
        | "routeDeleted"
        | "domainMoved"
    }
  >,
): string {
  switch (warning.code) {
    case "taskImageCopied":
      return t("warn.taskImageCopied", warning.from.name, warning.image)
    case "permanentRedirect":
      return t("warn.permanentRedirect", warning.hostname)
    case "passwordOverHttp":
      return t("warn.passwordOverHttp", t("common.list", warning.hostnames))
    case "publicPort":
      return warning.dataStore
        ? t("warn.publicDatabase", warning.workload.name)
        : t("warn.publicPort", warning.port)
    case "publicPortBlocked":
      return t("warn.publicPortBlocked", warning.port, t("common.list", warning.policies))
    default:
      return routeWarning(t, warning)
  }
}
