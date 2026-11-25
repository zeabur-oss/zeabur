// Installing or removing a catalog component: the plan's steps, then each step applied in turn
// while the page waits for the one before it to be ready.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useEffect, useState } from "react"

import type { Schemas } from "@/api/client"
import { applyComponent, planComponent } from "@/api/zeabur"
import { capabilitiesQuery, refresh, reviewOnce } from "@/api/queries"
import { Command } from "@/components/bits"
import { InputsForm, InputsReview, isMissing, type Values } from "@/components/component-inputs"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { useNow } from "@/lib/page-visible"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"

/** How long the page waits before it asks the server to go on. */
const POLL_MS = 3_000

const health = {
  pending: "idle",
  waiting: "progress",
  ready: "ok",
  failed: "fail",
} as const

const planQuery = (id: string, action: Schemas.ComponentAction, values: Values = {}) => ({
  queryKey: ["component-plan", id, action, values],
  queryFn: ({ signal }: { signal: AbortSignal }) =>
    planComponent(id, { action, values }, { signal }),
  ...reviewOnce,
})

/** Installs the component until its install plan is done, then removes it. The server's plan
 * decides, since only it sees every object Zeabur wrote; the action stays while the dialog is
 * open, so a failed install is tried again rather than removed. Once Zeabur wrote a step, the
 * install dialog also offers removal, for an install that cannot finish. An installed component
 * is offered removal when its install plan is refused, such as when a dependency was removed,
 * and new values when its component asks for any. */
export function ComponentPlan({
  id,
  name,
  installed,
  onClose,
}: {
  id: string
  name: string
  installed: boolean
  onClose: () => void
}) {
  const install = useQuery(planQuery(id, "install"))
  const [action, setAction] = useState<Schemas.ComponentAction>()
  const [editing, setEditing] = useState(false)
  if (action === undefined && install.data) setAction(install.data.done ? "remove" : "install")
  const changeValues = () => {
    setEditing(true)
    setAction("install")
  }
  if (action)
    return (
      <PlanDialog
        key={action}
        id={id}
        name={name}
        action={action}
        editing={editing}
        onClose={onClose}
        onRemove={action === "install" ? () => setAction("remove") : undefined}
        onEdit={action === "remove" && install.data?.inputs.length ? changeValues : undefined}
      />
    )
  return (
    <PlanLoading
      id={id}
      name={name}
      installed={installed}
      error={install.isError ? install.error : undefined}
      onClose={onClose}
      onRemove={() => setAction("remove")}
    />
  )
}

/** The install plan while it loads, or why it was refused. */
function PlanLoading({
  id,
  name,
  installed,
  error,
  onClose,
  onRemove,
}: {
  id: string
  name: string
  installed: boolean
  error: Error | undefined
  onClose: () => void
  onRemove: () => void
}) {
  const t = useT()
  return (
    <Frame
      title={t("installer.installTitle", name)}
      wide
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          {installed && error ? (
            <Button variant="outline" size="sm" onClick={onRemove}>
              {t("settings.removeCommand")}
            </Button>
          ) : null}
        </>
      }
    >
      {error ? (
        <div className="flex flex-col gap-5">
          <Problem error={error} />
          {installed ? null : <GitOpsExport id={id} action="install" />}
        </div>
      ) : (
        <LoadingRows label={t("plan.planning")} rows={3} />
      )}
    </Frame>
  )
}

/** An install whose component asks for values shows a field for each until none is missing and
 * the person checks the changes; every apply then sends the values the plan was made with. */
function PlanDialog({
  id,
  name,
  action,
  editing,
  onClose,
  onRemove,
  onEdit,
}: {
  id: string
  name: string
  action: Schemas.ComponentAction
  editing: boolean
  onClose: () => void
  onRemove?: () => void
  onEdit?: () => void
}) {
  const t = useT()
  const [values, setValues] = useState<Values>({})
  const plan = useQuery(planQuery(id, action, values))
  const { shown, running, start, apply } = useApplying(id, action, values, plan.data)
  const form = useInputs(shown, running, editing, setValues)
  return (
    <Frame
      title={t(titles[action], name)}
      wide
      onClose={onClose}
      footer={
        <Footer
          action={action}
          running={running}
          onClose={onClose}
          onStart={start}
          {...footerOf({
            shown,
            running,
            form,
            onEdit: action === "remove" ? onEdit : form.edit,
            onRemove,
          })}
        />
      }
    >
      {form.asking ? (
        <InputsForm
          inputs={form.inputs}
          values={form.draft}
          checked={form.checked}
          onChange={form.setDraft}
        />
      ) : null}
      <PlanContent
        name={name}
        pending={plan.isPending}
        error={plan.isError ? plan.error : undefined}
        plan={form.asking ? undefined : shown}
      />
      {apply.isError ? <Problem error={apply.error} /> : null}
      <GitOpsExport id={id} action={action} plan={shown} />
    </Frame>
  )
}

const titles = { install: "installer.installTitle", remove: "installer.removeTitle" } as const

/** The footer's plan and buttons: none of them change an install that runs or is done, and the
 * plan waits while the form asks for values. */
function footerOf({
  shown,
  running,
  form,
  onEdit,
  onRemove,
}: {
  shown: Schemas.ComponentPlan | undefined
  running: boolean
  form: { asking: boolean; check: () => void }
  onEdit?: () => void
  onRemove?: () => void
}) {
  const idle = !running && !shown?.done
  const written = shown?.steps.some((s) => s.state !== "pending") ?? false
  return {
    plan: form.asking ? undefined : shown,
    onCheck: form.asking ? form.check : undefined,
    onEdit: idle ? onEdit : undefined,
    onRemove: !running && written ? onRemove : undefined,
  }
}

/** The values a component asks for: the form shows while one is missing or the person changes
 * them, and checking the changes plans with them. */
function useInputs(
  shown: Schemas.ComponentPlan | undefined,
  running: boolean,
  editFirst: boolean,
  onCheck: (values: Values) => void,
) {
  const [draft, setDraft] = useState<Values>({})
  const [checked, setChecked] = useState(false)
  const [editing, setEditing] = useState(editFirst)
  const inputs = useLastInputs(shown)
  const wanted = editing || (isMissing(inputs) && !shown?.done)
  const asking = !running && wanted && inputs.length > 0
  const check = () => {
    onCheck(draft)
    setChecked(true)
    setEditing(false)
  }
  const edit = asking || inputs.length === 0 ? undefined : () => setEditing(true)
  return { inputs, draft, setDraft, checked, asking, check, edit }
}

/** The inputs of the last plan, kept while a new plan loads. */
function useLastInputs(shown: Schemas.ComponentPlan | undefined) {
  const [inputs, setInputs] = useState<Schemas.ComponentInput[]>([])
  const next = shown?.inputs
  if (next && next !== inputs && next.length + inputs.length > 0) setInputs(next)
  return inputs
}

function PlanContent({
  name,
  pending,
  error,
  plan,
}: {
  name: string
  pending: boolean
  error: Error | undefined
  plan: Schemas.ComponentPlan | undefined
}) {
  const t = useT()
  if (pending) return <LoadingRows label={t("plan.planning")} rows={3} />
  if (error) return <Problem error={error} />
  return plan ? <PlanBody name={name} plan={plan} /> : null
}

/** Applies the plan's next step, then asks again every few seconds until the plan is done or a
 * step or health check failed. An apply's plan shows only while the values it sent stay the same. */
function useApplying(
  id: string,
  action: Schemas.ComponentAction,
  values: Values,
  planned: Schemas.ComponentPlan | undefined,
) {
  const [running, setRunning] = useState(false)
  const [latest, setLatest] = useState<{ plan: Schemas.ComponentPlan; values: Values }>()
  const apply = useMutation({
    mutationFn: (version: string) =>
      applyComponent(id, { action, catalogVersion: version, values }),
    onSuccess: ({ plan: next }) => {
      setLatest({ plan: next, values })
      if (next.done || [...next.steps, ...next.checks].some((s) => s.state === "failed")) {
        setRunning(false)
        void refresh(capabilitiesQuery)
      }
    },
    onError: () => setRunning(false),
  })
  const shown = latest?.values === values ? latest.plan : planned
  useEffect(() => {
    if (!running || apply.isPending || !shown || shown.done) return
    const timer = setTimeout(() => apply.mutate(shown.catalogVersion), POLL_MS)
    return () => clearTimeout(timer)
  }, [running, apply, shown])

  const start = (version: string) => {
    setRunning(true)
    apply.mutate(version)
  }
  return { shown, running, start, apply }
}

/** The footer button of each action: before it starts, and while it runs. */
const labels = {
  install: ["settings.installCommand", "components.installing"],
  remove: ["settings.removeCommand", "components.removing"],
} as const

function Footer({
  action,
  plan,
  running,
  onClose,
  onStart,
  onCheck,
  onEdit,
  onRemove,
}: {
  action: Schemas.ComponentAction
  plan?: Schemas.ComponentPlan
  running: boolean
  onClose: () => void
  onStart: (version: string) => void
  onCheck?: () => void
  onEdit?: () => void
  onRemove?: () => void
}) {
  const t = useT()
  if (plan?.done)
    return (
      <Button size="sm" onClick={onClose}>
        {t("common.close")}
      </Button>
    )
  const install = action === "install"
  const [idle, busy] = labels[action]
  return (
    <>
      <Button variant="ghost" size="sm" onClick={onClose}>
        {t("plan.cancel")}
      </Button>
      {onRemove ? (
        <Button variant="outline" size="sm" onClick={onRemove}>
          {t("settings.removeCommand")}
        </Button>
      ) : null}
      {onEdit ? (
        <Button variant="outline" size="sm" onClick={onEdit}>
          {t("components.changeValues")}
        </Button>
      ) : null}
      {onCheck ? (
        <Button size="sm" onClick={onCheck}>
          {t("components.check")}
        </Button>
      ) : (
        <StartButton
          install={install}
          plan={plan}
          running={running}
          label={t(running ? busy : idle)}
          onStart={onStart}
        />
      )}
    </>
  )
}

function StartButton({
  install,
  plan,
  running,
  label,
  onStart,
}: {
  install: boolean
  plan?: Schemas.ComponentPlan
  running: boolean
  label: string
  onStart: (version: string) => void
}) {
  return (
    <Button
      size="sm"
      variant={install ? "default" : "destructive"}
      disabled={!plan || running}
      loading={running}
      onClick={() => plan && onStart(plan.catalogVersion)}
    >
      {label}
    </Button>
  )
}

/** The commands that print the component for Flux or Argo CD, which Zeabur does not install
 * through, until the install is done. */
function GitOpsExport({
  id,
  action,
  plan,
}: {
  id: string
  action: Schemas.ComponentAction
  plan?: Schemas.ComponentPlan
}) {
  const t = useT()
  if (action !== "install" || plan?.done) return null
  return (
    <section className="flex flex-col gap-2">
      <h3 className="type-meta font-medium text-muted-foreground">{t("components.gitops")}</h3>
      <p className="text-pretty text-muted-foreground">{t("components.gitopsHow")}</p>
      <Command text={`zeaburctl components export ${id} --format flux`} />
      <Command text={`zeaburctl components export ${id} --format argocd`} />
    </section>
  )
}

function PlanBody({ name, plan }: { name: string; plan: Schemas.ComponentPlan }) {
  const t = useT()
  return (
    <div className="flex flex-col gap-5">
      {plan.done ? (
        <Alert variant="info" role="status">
          <AlertDescription>
            {plan.action === "install" ? t("components.installed") : t("components.removed")}
          </AlertDescription>
        </Alert>
      ) : null}
      {plan.inputs.length > 0 ? <InputsReview inputs={plan.inputs} /> : null}
      {plan.warnings.map((warning) => (
        <Alert key={`${warning.kind}/${warning.component}`} variant="warn" role="note">
          <AlertDescription>
            {warning.kind === "untested" ? t("components.untested", warning.component) : null}
            {warning.kind === "privileged"
              ? t("components.privileged", warning.component, warning.level)
              : null}
            {warning.kind === "unchanged" ? (
              <>
                {t("components.unchanged", warning.component)}
                <YamlBlock yaml={warning.values} label={warning.component} className="mt-2" />
              </>
            ) : null}
          </AlertDescription>
        </Alert>
      ))}
      <section className="flex flex-col gap-2">
        <h3 className="type-meta font-medium text-muted-foreground">{t("components.steps")}</h3>
        <ol className="flex flex-col">
          {plan.steps.map((step, i) => (
            <Step
              // A removal can write a release again first and delete it last.
              key={`${i}/${step.component}/${step.release}`}
              step={step}
              removing={plan.action === "remove" ? name : undefined}
            />
          ))}
        </ol>
      </section>
      {plan.checks.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h3 className="type-meta font-medium text-muted-foreground">{t("components.checks")}</h3>
          <ul className="flex flex-col gap-2">
            {plan.checks.map((check) => (
              <Check key={`${check.component}/${check.name}`} check={check} />
            ))}
          </ul>
        </section>
      ) : null}
      {plan.leftovers.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h3 className="type-meta font-medium text-muted-foreground">
            {t("components.leftovers")}
          </h3>
          <p className="text-pretty text-muted-foreground">{t("components.leftoversWhy")}</p>
          <ul className="flex flex-col gap-1 font-mono text-sm" translate="no">
            {plan.leftovers.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  )
}

/** In a removal, a step that applies writes another component again without the values of the
 * one removed. */
function Step({ step, removing }: { step: Schemas.ComponentStep; removing?: string }) {
  const t = useT()
  const rewrite = removing !== undefined && step.writes.some((w) => w.op === "apply")
  return (
    <li className="flex flex-col gap-2 border-t border-rule py-3 first:border-t-0">
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 font-medium" translate="no">
          {step.component === step.release
            ? step.component
            : `${step.component}${t("common.sep")}${step.release}`}
        </span>
        <StatusPill health={health[step.state]} size="sm">
          {t(`components.state.${step.state}`)}
        </StatusPill>
      </div>
      {rewrite ? (
        <p className="text-pretty text-muted-foreground">{t("components.rewrite", removing)}</p>
      ) : null}
      {step.message ? <p className="text-pretty text-destructive">{step.message}</p> : null}
      <ul className="flex flex-col gap-1">
        {step.writes.map((write) => {
          const label = `${write.kind}/${write.name}`
          return (
            <li key={`${write.namespace ?? ""}/${label}`} className="flex flex-col gap-1">
              {write.yaml ? (
                <details>
                  <summary className="cursor-pointer text-sm">
                    <Written write={write} removal={removing !== undefined} />
                  </summary>
                  <YamlBlock yaml={write.yaml} label={label} className="mt-2" />
                </details>
              ) : (
                <span className="text-sm">
                  <Written write={write} removal={removing !== undefined} />
                </span>
              )}
            </li>
          )
        })}
      </ul>
    </li>
  )
}

function Written({ write, removal }: { write: Schemas.ObjectWrite; removal: boolean }) {
  const t = useT()
  return (
    <>
      {removal ? (
        <span className="mr-2 type-meta text-muted-foreground">
          {t(`components.op.${write.op}`)}
        </span>
      ) : null}
      <span>{kindName(t, write.kind)}</span>{" "}
      <span className="font-mono break-all" translate="no">
        {write.name}
      </span>
      {write.namespace ? (
        <span className="ml-2 type-meta text-muted-foreground" translate="no">
          {write.namespace}
        </span>
      ) : null}
    </>
  )
}

/** A check's timeout counts from when it starts to wait: a component's checks start once its own
 * steps are ready, while the steps after it still wait. */
function Check({ check }: { check: Schemas.ComponentCheck }) {
  const t = useT()
  const waiting = check.state === "waiting"
  const [waitingSince, setWaitingSince] = useState<number>()
  useEffect(() => {
    if (waiting && waitingSince === undefined) setWaitingSince(Date.now())
  }, [waiting, waitingSince])
  const now = useNow(5_000, waiting)
  const late =
    waiting && waitingSince !== undefined && now - waitingSince > check.timeoutSeconds * 1000
  return (
    <li className="flex flex-col gap-1">
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 font-mono text-sm" translate="no">
          {check.name}
        </span>
        <StatusPill health={health[check.state]} size="sm">
          {t(`components.state.${check.state}`)}
        </StatusPill>
      </div>
      {check.message ? (
        <p className="type-meta text-pretty break-words text-destructive" translate="no">
          {check.message}
        </p>
      ) : null}
      {late || check.state === "failed" ? (
        <p className="type-meta text-pretty text-muted-foreground">{check.advice}</p>
      ) : null}
    </li>
  )
}
