import { Link } from "@tanstack/react-router"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"
import WrenchIcon from "~icons/ph/wrench"

import type { Schemas } from "@/api/client"
import { usePlanner } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { serviceLink } from "@/lib/place"
import { fixLink, fixTemplates } from "@/model/fixes"
import { suggestionsOf } from "@/model/overview"

type Gap = Schemas["Gap"]

export function GapList({
  gaps,
  project,
  document,
}: {
  gaps: Gap[]
  /** Absent for cluster gaps. */
  project?: string
  document?: Schemas["Project"]
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
          const gap = row.gap
          const ask = document ? fixTemplates[gap.fix.id]?.(gap, document, t) : undefined
          return (
            <GapRow
              key={`${gap.rule}/${gap.subject.kind}/${gap.subject.name}/${gap.message.key}/${gap.message.params?.container ?? ""}`}
              gap={gap}
              onPlan={ask ? () => planner.plan(ask) : undefined}
              form={ask || !document ? undefined : fixLink(gap, document)}
            />
          )
        })}
      </ul>
      {planner.dialog}
    </>
  )
}

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
  const install = gap.fix.kind === "install" ? gap.fix.id.split("/") : undefined
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
        {install && install.length === 2 ? (
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
