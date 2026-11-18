// Installing or removing a catalog component: the plan's steps, then each step applied in turn
// while the page waits for the one before it to be ready.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useEffect, useState } from "react"

import type { Schemas } from "@/api/client"
import { applyComponent, planComponent } from "@/api/zeabur"
import { capabilitiesQuery, refresh, reviewOnce } from "@/api/queries"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
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

export function ComponentPlan({
  id,
  name,
  action,
  onClose,
}: {
  id: string
  name: string
  action: Schemas.ComponentAction
  onClose: () => void
}) {
  const t = useT()
  const plan = useQuery({
    queryKey: ["component-plan", id, action],
    queryFn: ({ signal }) => planComponent(id, { action }, { signal }),
    ...reviewOnce,
  })
  const [running, setRunning] = useState(false)
  const [latest, setLatest] = useState<Schemas.ComponentPlan>()
  const [readySince, setReadySince] = useState<number>()
  const apply = useMutation({
    mutationFn: (version: string) => applyComponent(id, { action, catalogVersion: version }),
    onSuccess: (next) => {
      setLatest(next)
      if (next.done || next.steps.some((s) => s.state === "failed")) {
        setRunning(false)
        void refresh(capabilitiesQuery)
      }
    },
    onError: () => setRunning(false),
  })
  const shown = latest ?? plan.data
  const stepsReady = shown?.steps.every((s) => s.state === "ready") ?? false

  useEffect(() => {
    if (stepsReady && readySince === undefined) setReadySince(Date.now())
  }, [stepsReady, readySince])

  useEffect(() => {
    if (!running || apply.isPending || !shown || shown.done) return
    const timer = setTimeout(() => apply.mutate(shown.catalogVersion), POLL_MS)
    return () => clearTimeout(timer)
  }, [running, apply, shown])

  const title =
    action === "install" ? t("installer.installTitle", name) : t("installer.removeTitle", name)
  const verb = action === "install" ? t("settings.installCommand") : t("settings.removeCommand")
  return (
    <Frame
      title={title}
      wide
      onClose={onClose}
      footer={
        shown?.done ? (
          <Button size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("plan.cancel")}
            </Button>
            <Button
              size="sm"
              variant={action === "remove" ? "destructive" : "default"}
              disabled={!shown || running}
              loading={running}
              onClick={() => {
                if (!shown) return
                setRunning(true)
                apply.mutate(shown.catalogVersion)
              }}
            >
              {running
                ? t(action === "install" ? "components.installing" : "components.removing")
                : verb}
            </Button>
          </>
        )
      }
    >
      {plan.isPending ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} />
      ) : shown ? (
        <PlanBody plan={shown} readySince={readySince} />
      ) : null}
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}

function PlanBody({ plan, readySince }: { plan: Schemas.ComponentPlan; readySince?: number }) {
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
      {plan.warnings.map((warning) => (
        <Alert key={`${warning.kind}/${warning.component}`} variant="warn" role="note">
          <AlertDescription>
            {warning.kind === "untested"
              ? t("components.untested", warning.component)
              : t("components.privileged", warning.component, warning.level)}
          </AlertDescription>
        </Alert>
      ))}
      <section className="flex flex-col gap-2">
        <h3 className="type-meta font-medium text-muted-foreground">{t("components.steps")}</h3>
        <ol className="flex flex-col">
          {plan.steps.map((step) => (
            <Step key={`${step.component}/${step.release}`} step={step} />
          ))}
        </ol>
      </section>
      {plan.checks.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h3 className="type-meta font-medium text-muted-foreground">{t("components.checks")}</h3>
          <ul className="flex flex-col gap-2">
            {plan.checks.map((check) => (
              <Check
                key={`${check.component}/${check.name}`}
                check={check}
                readySince={readySince}
              />
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

function Step({ step }: { step: Schemas.ComponentStep }) {
  const t = useT()
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
      {step.message ? <p className="text-pretty text-destructive">{step.message}</p> : null}
      <ul className="flex flex-col gap-1">
        {step.writes.map((write) => {
          const label = `${write.kind}/${write.name}`
          return (
            <li key={`${write.namespace ?? ""}/${label}`} className="flex flex-col gap-1">
              {write.yaml ? (
                <details>
                  <summary className="cursor-pointer text-sm">
                    <Written write={write} />
                  </summary>
                  <YamlBlock yaml={write.yaml} label={label} className="mt-2" />
                </details>
              ) : (
                <span className="text-sm">
                  <Written write={write} />
                </span>
              )}
            </li>
          )
        })}
      </ul>
    </li>
  )
}

function Written({ write }: { write: Schemas.ObjectWrite }) {
  const t = useT()
  return (
    <>
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

function Check({ check, readySince }: { check: Schemas.ComponentCheck; readySince?: number }) {
  const t = useT()
  const [now, setNow] = useState(Date.now)
  const waiting = check.state === "waiting"
  useEffect(() => {
    if (!waiting) return
    const timer = setInterval(() => setNow(Date.now()), 5_000)
    return () => clearInterval(timer)
  }, [waiting])
  const late = waiting && readySince !== undefined && now - readySince > check.timeoutSeconds * 1000
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
      {late ? <p className="type-meta text-pretty text-muted-foreground">{check.advice}</p> : null}
    </li>
  )
}
