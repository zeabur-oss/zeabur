// The pieces of a cluster-wide plan dialog (a node terminal, the cluster's kubectl terminal): its
// writes, the bindings that stop it, and a footer that applies the plan until its token runs out.
import CaretRightIcon from "~icons/ph/caret-right"
import WarningCircleIcon from "~icons/ph/warning-circle"

import type { Schemas } from "@/api/client"
import { COUNTDOWN_FROM_SECONDS, clock, useSecondsLeft } from "@/components/plan/expiry"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { subjectName } from "@/model/node-shell"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

export function PlanFooter({
  expiresAt,
  ready,
  token,
  action,
  destructive = false,
  applying,
  onCancel,
  onReplan,
  onApply,
}: {
  expiresAt: string | undefined
  /** Planned, and not planning again. */
  ready: boolean
  token: string | undefined
  action: string
  destructive?: boolean
  applying: boolean
  onCancel: () => void
  onReplan: () => void
  onApply: (token: string) => void
}) {
  const t = useT()
  const left = useSecondsLeft(expiresAt)
  const expired = left !== undefined && left <= 0
  return (
    <div className="flex w-full flex-wrap items-center gap-2">
      <Button variant="ghost" size="sm" onClick={onCancel}>
        {t("plan.cancel")}
      </Button>
      {ready && token && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
        <span className="type-meta text-muted-foreground tabular-nums">
          {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
        </span>
      ) : null}
      <span className="ml-auto">
        {expired || (ready && !token) ? (
          <Button size="sm" variant="outline" onClick={onReplan}>
            {t("plan.replan")}
          </Button>
        ) : (
          <Button
            size="sm"
            variant={destructive ? "destructive" : "default"}
            disabled={!ready || !token}
            loading={applying}
            onClick={() => token && onApply(token)}
          >
            {action}
          </Button>
        )}
      </span>
    </div>
  )
}

export function PlanWrites({ writes }: { writes: Schemas["ShellWrite"][] }) {
  const t = useT()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("plan.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-4 pt-3">
          {writes.map((write) => {
            const label = `${write.kind}/${write.name}`
            return (
              <section key={label} aria-label={label} className="flex min-w-0 flex-col gap-2">
                <span className="min-w-0 font-mono text-sm font-medium break-all" translate="no">
                  {label}
                  <span className="ml-2 font-sans font-normal type-meta text-muted-foreground">
                    {t(`nodeShell.write.${write.action}`)}
                  </span>
                </span>
                {write.yaml ? <YamlBlock yaml={write.yaml} label={label} /> : null}
              </section>
            )
          })}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

/** Someone besides cluster administrators could get into the pod: the plan stops here. */
export function Exposed({
  namespace,
  exposed,
}: {
  namespace: string
  exposed: Schemas["ShellExposure"][]
}) {
  const t = useT()
  return (
    <Alert variant="destructive">
      <WarningCircleIcon aria-hidden="true" />
      <AlertTitle>{t("nodeShell.exposedTitle")}</AlertTitle>
      <AlertDescription>
        <p>{t("nodeShell.exposed", namespace)}</p>
        <ul className="mt-2 flex flex-col gap-2">
          {exposed.map((binding) => (
            <li key={`${binding.kind}/${binding.name}`} className="flex flex-col gap-0.5">
              <span className="font-mono text-xs break-all" translate="no">
                {`${binding.kind}/${binding.name} → ${binding.role}`}
              </span>
              <span className="type-meta">
                <span translate="no">{binding.rights.join(", ")}</span>
                {t("common.sep")}
                <span translate="no">
                  {binding.subjects
                    .map((subject) => `${subject.kind} ${subjectName(subject)}`)
                    .join(", ")}
                </span>
              </span>
            </li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  )
}
