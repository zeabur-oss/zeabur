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
import { YamlBlock } from "@/components/yaml"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { fixTemplates } from "@/model/fixes"

type Gap = Schemas["GapView"]

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
        {gaps.map((gap) => {
          // A template can decline a case (no numeric port to check): the snippet stays the fix.
          const ask = project && document ? fixTemplates[gap.fix.id]?.(gap, document, t) : undefined
          return (
            <GapRow
              key={`${gap.rule}/${gap.subject.kind}/${gap.subject.name}/${gap.message.key}/${gap.message.params?.container ?? ""}`}
              gap={gap}
              onPlan={ask ? () => planner.plan(ask) : undefined}
            />
          )
        })}
      </ul>
      {planner.dialog}
    </>
  )
}

function GapRow({ gap, onPlan }: { gap: Gap; onPlan?: () => void }) {
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
      {gap.snippet && !onPlan ? (
        <Collapsible>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("gap.snippet")}
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="pt-2">
              <YamlBlock yaml={gap.snippet} label={t("gap.snippet")} />
            </div>
          </CollapsibleContent>
        </Collapsible>
      ) : null}
    </li>
  )
}
