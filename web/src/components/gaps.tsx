import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"
import WarningIcon from "~icons/ph/warning"
import WrenchIcon from "~icons/ph/wrench"

import type { Schemas } from "@/api/client"
import { projectGapsQuery, projectQuery } from "@/api/queries"
import { usePlanner } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { Button, buttonVariants } from "@zeabur-oss/design-system/components/button"
import { Drawer } from "@zeabur-oss/design-system/components/drawer"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { serviceLink } from "@/lib/place"
import { fixOf, type fixLink } from "@/model/fixes"
import { suggestionsOf, warningsAbout } from "@/model/overview"

type Gap = Schemas.Gap

export function GapList({
  gaps,
  project,
  document,
}: {
  gaps: Gap[]
  /** Absent for cluster gaps. */
  project?: string
  document?: Schemas.Project
}) {
  const t = useT()
  const planner = usePlanner(project ?? "")
  return (
    <>
      <ul className="flex flex-col">
        {suggestionsOf(gaps).map((row) => {
          if (row.as === "unchecked")
            return project ? (
              <UncheckedRow key="unchecked" project={project} services={row.services} />
            ) : null
          const { ask, form } = fixOf(row.gap, document, t)
          return (
            <GapRow
              key={gapKey(row.gap)}
              gap={row.gap}
              onPlan={ask ? () => planner.plan({ ...ask, via: "suggestion" }) : undefined}
              form={form}
            />
          )
        })}
      </ul>
      {planner.dialog}
    </>
  )
}

/** A warning mark on a map node's corner; it opens the node's warnings with their fixes. */
export function NodeWarnings({
  project,
  object,
}: {
  project: string
  object: { kind: string; name: string }
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const gaps = useQuery(projectGapsQuery(project)).data?.items ?? []
  const document = useQuery(projectQuery(project)).data
  const warnings = warningsAbout(gaps, object)
  if (warnings.length === 0) return null
  const label = t("gap.nodeWarnings", warnings.length)
  return (
    <>
      <Tooltip content={label}>
        <button
          type="button"
          aria-label={label}
          onClick={() => setOpen(true)}
          className="absolute -top-2 -right-2 z-2 flex size-6 items-center justify-center rounded-full border border-warn/40 bg-card text-warn shadow-xs hover:bg-warn-soft"
        >
          <WarningIcon aria-hidden="true" className="size-3.5" />
        </button>
      </Tooltip>
      <Drawer
        open={open}
        onOpenChange={setOpen}
        title={t("gap.title")}
        description={<span translate="no">{object.name}</span>}
        closeLabel={t("common.close")}
      >
        <GapList gaps={warnings} project={project} document={document} />
      </Drawer>
    </>
  )
}

const gapKey = (gap: Gap) =>
  `${gap.rule}/${gap.subject.kind}/${gap.subject.name}/${gap.message.key}/${gap.message.params?.container ?? ""}`

/** Every service without a traffic check, as one piece of advice; the names open below it. */
function UncheckedRow({
  project,
  services,
}: {
  project: string
  services: { kind: string; name: string }[]
}) {
  const t = useT()
  return (
    <li className="flex flex-col gap-1.5 border-t border-rule py-3 first:border-t-0">
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <StatusPill health="idle" size="sm">
          {t("gap.advice")}
        </StatusPill>
        <span className="min-w-0 flex-1 text-pretty">{t("gap.unchecked", services.length)}</span>
      </div>
      <p className="type-meta text-muted-foreground">{t("gap.uncheckedWhy")}</p>
      <Collapsible>
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("gap.uncheckedNames")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <ul className="flex flex-wrap gap-x-3 gap-y-1 pt-2 type-meta">
            {services.map((service) => (
              <li key={`${service.kind}/${service.name}`}>
                <Link
                  {...serviceLink(project, service, { tab: "health-checks" })}
                  className="type-mono text-foreground hover:underline"
                  translate="no"
                >
                  {service.name}
                </Link>
              </li>
            ))}
          </ul>
        </CollapsibleContent>
      </Collapsible>
    </li>
  )
}

function GapRow({
  gap,
  onPlan,
  form,
}: {
  gap: Gap
  onPlan?: () => void
  form?: ReturnType<typeof fixLink>
}) {
  const t = useT()
  const write = useSentence()
  return (
    <li className="flex flex-col gap-1.5 border-t border-rule py-3 first:border-t-0">
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <StatusPill health={gap.severity === "warning" ? "warn" : "idle"} size="sm">
          {t(gap.severity === "warning" ? "gap.warning" : "gap.advice")}
        </StatusPill>
        <span className="min-w-0 flex-1 text-pretty">{write(gap.message)}</span>
        {onPlan ? (
          <Button variant="outline" size="sm" onClick={onPlan}>
            <WrenchIcon aria-hidden="true" data-icon="inline-start" />
            {t("gap.fix")}
          </Button>
        ) : null}
        {form ? (
          <Link {...form.link} className={cn(buttonVariants({ variant: "outline", size: "sm" }))}>
            {t(form.label)}
          </Link>
        ) : null}
        {gap.fix.kind === "install" ? (
          <Link
            to="/cluster/components"
            search={{ component: gap.fix.id }}
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          >
            {t("gap.install")}
          </Link>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 type-meta text-muted-foreground">
        <span
          className="max-w-full min-w-0 truncate type-mono"
          translate="no"
          title={`${gap.subject.kind}/${gap.subject.name}`}
        >
          {gap.subject.kind}/{gap.subject.name}
        </span>
        {gap.citations.map((c) => (
          <a
            key={c.url}
            href={c.url}
            target="_blank"
            rel="noreferrer noopener"
            className="inline-flex items-center gap-0.5 hover:text-foreground hover:underline"
          >
            {c.title}
            <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
            <span className="sr-only">{t("settings.opensInNewTab")}</span>
          </a>
        ))}
      </div>
    </li>
  )
}
