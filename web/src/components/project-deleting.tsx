// A project whose Namespace is Terminating: what is left and, when finalizers hold objects, which
// ones, how to look at them, and Force delete, a plan that removes those finalizers.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import CaretRightIcon from "~icons/ph/caret-right"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { namespaceQuery } from "@/api/queries"
import { usePlanner } from "@/components/plan/planner"
import { YamlBlock } from "@/components/yaml"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useI18n, useT, type Translate } from "@/i18n"
import { formatAgo } from "@/lib/format"
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
    <Alert variant={holding ? "warn" : "info"} role="status" className={className}>
      <TrashIcon aria-hidden="true" />
      <AlertTitle>{t("deleting.title")}</AlertTitle>
      <AlertDescription>
        <p>{state ? progressText(t, locale, state) : t("deleting.body")}</p>
        {state && holding ? <Held project={project} held={state.held} /> : null}
      </AlertDescription>
    </Alert>
  )
}

/** Why the project is held, Force delete when Zeabur can lift it, and the kubectl commands. */
function Held({ project, held }: { project: string; held: Schemas.HeldObject[] }) {
  const t = useT()
  const forcible = held.length > 0
  return (
    <>
      <p>
        {t("deleting.held")}
        {forcible ? `${t("common.gap")}${t("deleting.forceHint")}` : ""}
      </p>
      <div className="mt-3 flex min-w-0 flex-col items-start gap-2">
        {forcible ? <ForceDelete project={project} /> : null}
        <HeldObjects project={project} held={held} />
      </div>
    </>
  )
}

function progressText(t: Translate, locale: string, state: Schemas.ProjectDeleting): string {
  const left = state.remaining.map((r) =>
    t("deleting.count", r.group ? `${r.resource}.${r.group}` : r.resource, r.count),
  )
  return [
    t("deleting.body"),
    t("deleting.since", formatAgo(state.since, locale)),
    left.length > 0 ? t("deleting.left", t("common.list", left)) : "",
  ]
    .filter(Boolean)
    .join(t("common.gap"))
}

function ForceDelete({ project }: { project: string }) {
  const t = useT()
  const planner = usePlanner(project, undefined, true)
  return (
    <>
      <Button
        variant="outline"
        size="sm"
        onClick={() =>
          planner.plan({
            title: t("title.forceDeleteProject", project),
            changes: [{ op: "forceDeleteProject" }],
          })
        }
      >
        {t("deleting.force")}
      </Button>
      {planner.dialog}
    </>
  )
}

function HeldObjects({ project, held }: { project: string; held: Schemas.HeldObject[] }) {
  const t = useT()
  return (
    <Collapsible className="w-full min-w-0">
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
