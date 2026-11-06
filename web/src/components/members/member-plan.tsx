import { useMutation, useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import CaretLeftIcon from "~icons/ph/caret-left"

import type { Schemas } from "@/api/client"
import {
  applyMemberChange,
  planMemberChange,
  refreshMembers,
  type MemberChange,
} from "@/api/members"
import { WrittenList } from "@/components/cluster-plan"
import { DescribeYaml } from "@/components/describe-yaml"
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
import { Button } from "@zeabur-oss/design-system/components/button"

interface PlanCopy {
  title: string
  consequence?: string
  lines: string[]
  action: string
  destructive?: boolean
  /** Back to the form; Cancel when there is none. */
  onBack?: () => void
  onClose: () => void
}

/** A change to the members as its plan: what it does in words first, the objects it writes behind
 * Technical details, and the button that applies exactly that. */
export function MemberPlan({
  change,
  onApplied,
  ...copy
}: PlanCopy & { change: MemberChange; onApplied: (applied: Schemas.MemberApplied) => void }) {
  return (
    <WritesPlan
      {...copy}
      queryKey={["member-plan", change]}
      plan={(signal) => planMemberChange(change, signal)}
      apply={applyMemberChange}
      refresh={refreshMembers}
      onApplied={onApplied}
    />
  )
}

/** A sealed plan of object writes, members' or API tokens': planned as a query, applied by its
 * token. */
export function WritesPlan<Applied>({
  queryKey,
  plan: planOf,
  apply: applyToken,
  refresh,
  title,
  consequence,
  lines,
  action,
  destructive,
  onBack,
  onClose,
  onApplied,
}: PlanCopy & {
  queryKey: readonly unknown[]
  plan: (
    signal: AbortSignal,
  ) => Promise<{ token: string; expiresAt: string; writes: MemberWrite[] }>
  apply: (token: string) => Promise<Applied>
  refresh: () => void
  onApplied: (applied: Applied) => void
}) {
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey,
    queryFn: ({ signal }) => planOf(signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyToken(token),
    onSuccess: (applied) => {
      refresh()
      onApplied(applied)
    },
    onError: refresh,
  })
  const writes = plan.data?.writes ?? []
  return (
    <Frame
      title={title}
      wide
      onClose={onClose}
      footer={
        <PlanFooter
          expiresAt={plan.data?.expiresAt}
          ready={plan.data !== undefined && !plan.isFetching}
          empty={writes.length === 0}
          applying={apply.isPending}
          destructive={destructive}
          action={action}
          onReplan={() => void plan.refetch()}
          onApply={() => plan.data && apply.mutate(plan.data.token)}
          back={<BackButton onBack={onBack} onClose={onClose} />}
        />
      }
    >
      <PlanBody
        loading={plan.isPending || plan.isFetching}
        error={plan.error}
        consequence={consequence}
        lines={lines}
        writes={writes}
      />
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}

function PlanBody({
  loading,
  error,
  ...said
}: {
  loading: boolean
  error: Error | null
  consequence?: string
  lines: string[]
  writes: MemberWrite[]
}) {
  const t = useT()
  if (loading) return <LoadingRows label={t("plan.planning")} rows={3} />
  if (error) return <Problem error={error} />
  return <Said {...said} />
}

function PlanFooter({
  expiresAt,
  ready,
  empty,
  applying,
  destructive,
  action,
  onReplan,
  onApply,
  back,
}: {
  expiresAt: string | undefined
  ready: boolean
  empty: boolean
  applying: boolean
  destructive?: boolean
  action: string
  onReplan: () => void
  onApply: () => void
  back: ReactNode
}) {
  const left = useSecondsLeft(expiresAt)
  return (
    <SealedFooter left={left} ready={ready} onReplan={onReplan} back={back}>
      <Button
        size="sm"
        variant={destructive ? "destructive" : "default"}
        disabled={!ready || empty}
        loading={applying}
        onClick={onApply}
      >
        {action}
      </Button>
    </SealedFooter>
  )
}

function BackButton({ onBack, onClose }: { onBack?: () => void; onClose: () => void }) {
  const t = useT()
  return onBack ? (
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

/** The plan in words, then its writes; "Nothing changes" when it writes nothing. */
function Said({
  consequence,
  lines,
  writes,
}: {
  consequence?: string
  lines: string[]
  writes: MemberWrite[]
}) {
  const t = useT()
  return (
    <>
      {consequence ? <Consequence>{consequence}</Consequence> : null}
      {writes.length === 0 ? (
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
    <DescribeYaml
      describe={
        <div className="flex flex-col gap-2">
          <p className="text-sm">{said.join(t("common.sep"))}</p>
          <WrittenList
            items={writes.map((write) => ({
              kind: write.kind,
              name: writtenName(write),
              state:
                write.action === "create"
                  ? t("plan.create")
                  : write.action === "delete"
                    ? t("plan.delete")
                    : t("members.plan.changed"),
              place: write.namespace ?? undefined,
            }))}
          />
        </div>
      }
      yaml={
        <div className="flex flex-col gap-4">
          {writes.map((write, i) => (
            <Written
              key={`${i}-${write.kind}/${write.namespace ?? ""}/${write.name}`}
              write={write}
            />
          ))}
        </div>
      }
    />
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
