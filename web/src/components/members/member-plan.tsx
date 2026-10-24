import { useMutation, useQuery } from "@tanstack/react-query"
import CaretLeftIcon from "~icons/ph/caret-left"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import {
  applyMemberChange,
  planMemberChange,
  refreshMembers,
  type MemberChange,
} from "@/api/members"
import { Consequence, Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { useSecondsLeft } from "@/components/plan/expiry"
import { SealedFooter } from "@/components/plan/sealed-footer"
import { Problem } from "@/components/problem"
import { YamlDiff } from "@/components/yaml"
import { projectsQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { writeCounts, writtenName, type MemberWrite } from "@/model/members"
import { projectMention } from "@/model/project"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

/** A change to the members as its plan: what it does in words first, the objects it writes behind
 * Technical details, and the button that applies exactly that. */
export function MemberPlan({
  title,
  change,
  consequence,
  lines,
  action,
  destructive,
  onBack,
  onClose,
  onApplied,
}: {
  title: string
  change: MemberChange
  consequence?: string
  lines: string[]
  action: string
  destructive?: boolean
  /** Back to the form; Cancel when there is none. */
  onBack?: () => void
  onClose: () => void
  onApplied: (applied: Schemas["MemberApplied"]) => void
}) {
  const t = useT()
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey: ["member-plan", change],
    queryFn: ({ signal }) => planMemberChange(change, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyMemberChange(token),
    onSuccess: (applied) => {
      refreshMembers()
      onApplied(applied)
    },
    onError: refreshMembers,
  })
  const left = useSecondsLeft(plan.data?.expiresAt)
  const writes = plan.data?.writes ?? []
  const nothing = plan.data !== undefined && writes.length === 0
  const ready = plan.data && !plan.isFetching
  return (
    <Frame
      title={title}
      wide
      onClose={onClose}
      footer={
        <SealedFooter
          left={left}
          ready={Boolean(ready)}
          onReplan={() => void plan.refetch()}
          back={
            onBack ? (
              <Button variant="outline" size="sm" onClick={onBack}>
                <CaretLeftIcon aria-hidden="true" data-icon="inline-start" />
                {t("plan.back")}
              </Button>
            ) : (
              <Button variant="ghost" size="sm" onClick={onClose}>
                {t("plan.cancel")}
              </Button>
            )
          }
        >
          <Button
            size="sm"
            variant={destructive ? "destructive" : "default"}
            disabled={!ready || nothing}
            loading={apply.isPending}
            onClick={() => plan.data && apply.mutate(plan.data.token)}
          >
            {action}
          </Button>
        </SealedFooter>
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} />
      ) : (
        <>
          {consequence ? <Consequence>{consequence}</Consequence> : null}
          {nothing ? (
            <p>{t("members.plan.nothing")}</p>
          ) : (
            lines.map((line, i) => (
              <p key={line} className={i > 0 || consequence ? "text-muted-foreground" : undefined}>
                {line}
              </p>
            ))
          )}
          {writes.length > 0 ? <Writes writes={writes} /> : null}
        </>
      )}
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}

/** A project as a plan's sentence names it: "Shop (shop)", or "blog" when it has no other name. */
export function useProjectMention(): (project: string) => string {
  const t = useT()
  const projects = useQuery(projectsQuery)
  return (project) => {
    const { title, namespace } = projectMention(
      projects.data?.items.find((item) => item.name === project),
      project,
    )
    return namespace ? t("members.plan.project", title, namespace) : title
  }
}

function Writes({ writes }: { writes: MemberWrite[] }) {
  const t = useT()
  const counts = writeCounts(writes)
  const said = [
    counts.create > 0 ? t("members.plan.creates", counts.create) : "",
    counts.patch > 0 ? t("members.plan.changes", counts.patch) : "",
    counts.delete > 0 ? t("members.plan.deletes", counts.delete) : "",
  ].filter(Boolean)
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure" className="mt-1">
        <CaretRightIcon aria-hidden="true" />
        {t("plan.details")}
        <span className="font-normal text-muted-foreground">{said.join(t("common.sep"))}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-4 pt-3">
          {writes.map((write, i) => (
            <Written
              key={`${i}-${write.kind}/${write.namespace ?? ""}/${write.name}`}
              write={write}
            />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function Written({ write }: { write: MemberWrite }) {
  const t = useT()
  const mention = useProjectMention()
  const name = `${write.kind}/${writtenName(write)}`
  const label =
    write.action === "create"
      ? t("plan.create")
      : write.action === "delete"
        ? t("plan.delete")
        : t("members.plan.changed")
  const shown = write.beforeYaml != null || write.afterYaml != null
  return (
    <section aria-label={name} className="flex min-w-0 flex-col gap-2">
      <header className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <span className="min-w-0 font-mono text-sm font-medium break-all" translate="no">
          {name}
        </span>
        {write.namespace ? (
          <span className="type-meta text-muted-foreground" translate="no">
            {t("members.plan.in", mention(write.namespace))}
          </span>
        ) : null}
        <span className="type-meta text-muted-foreground">{label}</span>
      </header>
      {shown ? (
        <YamlDiff before={write.beforeYaml ?? ""} after={write.afterYaml ?? ""} />
      ) : (
        <p className="type-meta text-muted-foreground">{t("members.plan.notShown")}</p>
      )}
    </section>
  )
}
