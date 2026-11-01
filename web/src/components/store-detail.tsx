// Parts of the pages of what an operator runs: databases and object storage.
import { useQuery } from "@tanstack/react-query"
import { useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import PlusIcon from "~icons/ph/plus"

import { toProblem, type Problem as ProblemBody, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { objectQuery, type ObjectAddress } from "@/api/queries"
import { CopyText } from "@/components/copy-text"
import { EventList } from "@/components/events"
import { GateTip, useGate } from "@/components/gate"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { YamlView } from "@/components/yaml-view"
import { useT } from "@/i18n"
import type { Gate } from "@/model/rights"
import { isWarning, type Event } from "@/model/service"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Container } from "@zeabur-oss/design-system/components/container"

export function Warnings({ events, onEvents }: { events: Event[]; onEvents: () => void }) {
  const t = useT()
  const warnings = events.filter(isWarning)
  if (warnings.length === 0) return null
  return (
    <Container
      title={t("overview.warnings")}
      titleAction={
        <button
          type="button"
          onClick={onEvents}
          className="flex items-center gap-0.5 rounded text-xs font-medium text-secondary outline-none hover:text-main"
        >
          {t("common.all")}
          <CaretRightIcon className="size-3.5" aria-hidden="true" />
        </button>
      }
      variant="list"
    >
      <div className="px-4">
        <EventList events={warnings} limit={3} compact plain />
      </div>
    </Container>
  )
}

export function Section({
  id,
  title,
  action,
  children,
}: {
  id: string
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-2">
      <div className="flex min-h-7 items-center justify-between gap-3">
        <h2 id={id} className="type-heading">
          {title}
        </h2>
        {action}
      </div>
      {children}
    </section>
  )
}

/** Rows in a card, labels and values at the summary card's facts' x. */
export function CardRows({ children }: { children: ReactNode }) {
  return (
    <dl className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
      {children}
    </dl>
  )
}

export function CardRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 items-center gap-x-4 px-4 py-1.5">
      <dt className="w-fact-label shrink-0 type-meta text-muted-foreground">{label}</dt>
      <dd className="flex min-h-8 min-w-0 flex-1 items-center gap-2">{children}</dd>
    </div>
  )
}

/** A Secret's values, read as the person only when they ask. */
export function useReveal(project: string, name: string) {
  const revealGate = useGate(project, "readSecrets")
  const [secret, setSecret] = useState<{
    pending?: boolean
    values?: Schemas.RevealedSecret["values"]
    problem?: ProblemBody
  }>()
  const reveal = async () => {
    setSecret({ pending: true })
    try {
      const answer = await zeabur.revealSecret(project, name, { cache: "no-store" })
      setSecret({ values: answer.values })
    } catch (error) {
      setSecret({ problem: toProblem(error) })
    }
  }
  return { secret, reveal, hide: () => setSecret(undefined), revealGate }
}

export const Mono = ({ text }: { text: string }) => (
  <span className="min-w-0 truncate type-mono" translate="no">
    {text}
  </span>
)

/** A Secret's value: hidden until the person reveals it. */
export function RevealRow({
  label,
  value,
  reveal: { secret, reveal, hide, revealGate },
}: {
  label: string
  value: string | undefined
  reveal: ReturnType<typeof useReveal>
}) {
  const t = useT()
  return (
    <CardRow label={label}>
      <Mono text={value ?? "••••••••"} />
      {value !== undefined ? (
        <>
          <CopyText value={() => value} label={t("common.copy")} />
          <Button variant="ghost" size="xs" onClick={hide}>
            <EyeSlashIcon aria-hidden="true" data-icon="inline-start" />
            {t("db.hide")}
          </Button>
        </>
      ) : (
        <GateTip gate={revealGate}>
          <Button variant="ghost" size="xs" loading={secret?.pending} onClick={() => void reveal()}>
            <EyeIcon aria-hidden="true" data-icon="inline-start" />
            {t("db.reveal")}
          </Button>
        </GateTip>
      )}
    </CardRow>
  )
}

/** The operator's object as YAML, editable through a plan. */
export function ObjectYaml({ project, address }: { project: string; address: ObjectAddress }) {
  const t = useT()
  const object = useQuery(objectQuery(project, address))
  if (object.isPending) return <LoadingRows label={t("common.loading")} rows={8} />
  if (object.isError) return <Problem error={object.error} />
  return (
    <YamlView
      project={project}
      address={address}
      yaml={object.data.yaml}
      managedBy={object.data.provenance.tool}
    />
  )
}

/** An outline button that opens a form, closed when the person's role cannot use it. */
export function OpenButton({
  gate,
  label,
  size = "xs",
  form,
}: {
  gate: Gate
  label: string
  size?: "xs" | "sm"
  form: (close: () => void) => ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <GateTip gate={gate}>
        <Button variant="outline" size={size} onClick={() => setOpen(true)}>
          <PlusIcon aria-hidden="true" data-icon="inline-start" />
          {label}
        </Button>
      </GateTip>
      {open ? form(() => setOpen(false)) : null}
    </>
  )
}
