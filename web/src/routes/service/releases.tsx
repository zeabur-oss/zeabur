// A service's releases: every pod template its workload has run, newest first (a Deployment's
// ReplicaSets, a StatefulSet's or DaemonSet's ControllerRevisions). Any two compare as a YAML
// diff; Roll back plans a return to an older template, which the plan dialog explains (ConfigMap
// and Secret contents stay as they are). A workload a tool manages is rolled back there instead.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"

import type { Schemas } from "@/api/client"
import { releasesQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { YamlDiff } from "@/components/yaml"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { imageTag, isTask, type Service } from "@/model/service"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"

type Release = Schemas["WorkloadRelease"]

export function ReleasesTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const releases = useQuery(releasesQuery(project, service.kind, service.name))
  const planner = usePlanner(project)
  // The release compared with the current one; the newest older one until a person picks.
  const [picked, setPicked] = useState<number | null>(null)

  if (releases.isPending) return <LoadingRows label={t("common.loading")} rows={4} />
  if (releases.isError) return <Problem error={releases.error} />
  const items = releases.data.items
  if (items.length === 0)
    return (
      <Empty size="sm">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="empty" seed="releases" />
          </EmptyMedia>
          <EmptyDescription>
            {t(isTask(service.kind) ? "releases.noneTask" : "releases.none")}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )

  const current = items.find((r) => r.current) ?? items[0]
  const others = items.filter((r) => r !== current)
  const compared = others.find((r) => r.revision === picked) ?? others[0]
  const managed = service.provenance.tool

  return (
    <div className="flex flex-col gap-6">
      <ol
        className="flex flex-col rounded-xl border border-v2-border bg-card"
        aria-label={t("tab.releases")}
      >
        {items.map((release) => (
          <li
            key={release.name}
            className={cn(
              "flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t px-4 py-3 first:border-t-0",
              release === compared && "bg-muted/40",
            )}
          >
            <span className="w-12 font-mono text-sm font-semibold tabular-nums">
              #{release.revision}
            </span>
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className="flex min-w-0 flex-wrap items-center gap-2">
                {release.images.map((image) => (
                  <span
                    key={image}
                    title={image}
                    className="truncate font-mono text-xs"
                    translate="no"
                  >
                    {image.replace(/[:@][^:/@]+$/, "")}
                    <span className="rounded bg-muted px-1">:{imageTag(image)}</span>
                  </span>
                ))}
                {release.current ? <Badge variant="ok">{t("releases.current")}</Badge> : null}
              </span>
              <span className="flex flex-wrap items-center gap-x-2 type-meta text-muted-foreground">
                <Ago iso={release.created} />
                {release.changeCause ? (
                  <span className="truncate" title={release.changeCause}>
                    {release.changeCause}
                  </span>
                ) : null}
              </span>
            </span>
            {release.pods.total > 0 ? (
              <StatusPill
                health={release.pods.ready === release.pods.total ? "ok" : "warn"}
                size="sm"
              >
                {t("pod.readyCount", release.pods.ready, release.pods.total)}
              </StatusPill>
            ) : null}
            {release.current ? null : (
              <span className="flex gap-1.5">
                <Button
                  variant="ghost"
                  size="xs"
                  aria-pressed={release === compared}
                  onClick={() => setPicked(release.revision)}
                >
                  {t("releases.compare")}
                </Button>
                <RollbackButton
                  managed={managed}
                  onClick={() =>
                    planner.plan({
                      title: t("title.rollback", service.name, release.revision),
                      changes: [
                        {
                          op: "rollback",
                          workload: { kind: service.kind, name: service.name },
                          revision: release.revision,
                        },
                      ],
                    })
                  }
                />
              </span>
            )}
          </li>
        ))}
      </ol>
      {current && compared ? <Comparison from={compared} to={current} /> : null}
      {planner.dialog}
    </div>
  )
}

function RollbackButton({ managed, onClick }: { managed?: string | null; onClick: () => void }) {
  const t = useT()
  const button = (
    <Button variant="outline" size="xs" disabled={!!managed} onClick={onClick}>
      <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
      {t("releases.rollback")}
    </Button>
  )
  return managed ? (
    <Tooltip content={t("common.managedByDetail", managed)}>
      <span>{button}</span>
    </Tooltip>
  ) : (
    button
  )
}

/** What changed from an older release to the current one. */
function Comparison({ from, to }: { from: Release; to: Release }) {
  const t = useT()
  const before = from.templateYaml
  const after = to.templateYaml
  return (
    <section className="flex flex-col gap-2" aria-labelledby="releases-diff">
      <h2 id="releases-diff" className="text-sm font-medium">
        {t("releases.diff", from.revision, to.revision)}
      </h2>
      {before === after ? (
        <p className="text-sm text-muted-foreground">{t("releases.same")}</p>
      ) : (
        <YamlDiff before={before} after={after} />
      )}
    </section>
  )
}
