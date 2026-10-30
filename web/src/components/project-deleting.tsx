// A project whose Namespace is Terminating: what is left and, when finalizers hold objects, which
// ones and how to look at them. Zeabur never removes a finalizer.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import CaretRightIcon from "~icons/ph/caret-right"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { namespaceQuery } from "@/api/queries"
import { YamlBlock } from "@/components/yaml"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useI18n, useT, type Translate } from "@/i18n"
import { formatAgo } from "@/lib/format"
import { cn } from "@/lib/utils"
import { useProjectSummary } from "@/lib/project-name"
import { deletingCommands } from "@/model/project-deletion"

const POLL_MS = 5_000

export function ProjectDeleting({ project, className }: { project: string; className?: string }) {
  const t = useT()
  const { locale } = useI18n()
  const deleting = useProjectSummary(project)?.deleting ?? false
  const namespace = useQuery({
    ...namespaceQuery(project),
    enabled: deleting,
    refetchInterval: POLL_MS,
    retry: false,
  })
  if (!deleting) return null
  if (namespace.isError)
    return (
      <Alert variant="default" role="status" className={className}>
        <TrashIcon aria-hidden="true" />
        <AlertTitle>{t("deleting.gone")}</AlertTitle>
        <AlertDescription>
          <Link to="/projects" className="text-foreground hover:underline">
            {t("deleting.toProjects")}
          </Link>
        </AlertDescription>
      </Alert>
    )
  const state = namespace.data?.deleting
  const holding = (state?.finalizers.length ?? 0) > 0
  return (
    <div className={cn("flex min-w-0 flex-col gap-3", className)}>
      <Alert variant={holding ? "warn" : "info"} role="status">
        <TrashIcon aria-hidden="true" />
        <AlertTitle>{t("deleting.title")}</AlertTitle>
        <AlertDescription>
          {state ? stateText(t, locale, state) : t("deleting.body")}
        </AlertDescription>
      </Alert>
      {holding ? <HeldObjects project={project} held={state?.held ?? []} /> : null}
    </div>
  )
}

function stateText(t: Translate, locale: string, state: Schemas.ProjectDeleting): string {
  const left = state.remaining.map((r) =>
    t("deleting.count", r.group ? `${r.resource}.${r.group}` : r.resource, r.count),
  )
  return [
    t("deleting.body"),
    t("deleting.since", formatAgo(state.since, locale)),
    left.length > 0 ? t("deleting.left", t("common.list", left)) : "",
    state.finalizers.length > 0 ? t("deleting.held") : "",
  ]
    .filter(Boolean)
    .join(t("common.gap"))
}

function HeldObjects({ project, held }: { project: string; held: Schemas.HeldObject[] }) {
  const t = useT()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("deleting.commandsLabel")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex min-w-0 flex-col gap-2 pt-2">
          <ul className="flex flex-col gap-0.5 type-mono">
            {held.map((h) => (
              <li key={`${h.resource}.${h.group}/${h.name}`} className="truncate" translate="no">
                {h.kind} {h.name}
                <span className="text-muted-foreground">
                  {t("common.sep")}
                  {h.finalizers.join(", ")}
                </span>
              </li>
            ))}
          </ul>
          <YamlBlock yaml={deletingCommands(project, held)} label={t("deleting.commandsLabel")} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
