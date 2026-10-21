// Protocol: crates/zeabur-api/src/tools.rs.
import { useMutation, useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { useEffect, useState } from "react"

import { api, unwrap } from "@/api/client"
import { queryClient, toolsQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { useI18n, useT } from "@/i18n"
import { formatAgo } from "@/lib/format"
import { ProjectHeader } from "@/routes/project/header"
import { Session } from "@/routes/service/terminal"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { PageColumn } from "@zeabur/design-system/components/page-column"

const route = getRouteApi("/projects/$project/terminal")

export function ToolsTerminalPage() {
  const { project } = route.useParams()
  const t = useT()
  const [waiting, setWaiting] = useState(false)
  const tools = useQuery({
    ...toolsQuery(project),
    refetchInterval: (query) =>
      waiting || query.state.data?.pods.some((p) => !p.ready) ? 1500 : 15_000,
  })
  const [open, setOpen] = useState<string>()
  const [attempt, setAttempt] = useState(0)
  const { plan, dialog } = usePlanner(project, () => setWaiting(true), true)
  const pods = tools.data?.pods ?? []
  const ready = pods.find((p) => p.ready)?.name
  useEffect(() => {
    if (!waiting || !ready) return
    setWaiting(false)
    setOpen(ready)
  }, [waiting, ready])
  const shown = pods.some((p) => p.name === open) ? open : undefined
  const failed = waiting ? pods.find((p) => p.problem)?.problem : undefined

  const start = () =>
    plan({
      title: t("action.openToolsTerminal"),
      changes: [{ op: "clusterTools" }],
      quiet: true,
    })

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ProjectHeader
        entry="terminal"
        description={tools.data?.available === false ? undefined : t("tools.about")}
        actions={
          tools.data?.available && !shown ? (
            <Button size="sm" onClick={start} disabled={waiting}>
              {waiting ? t("tools.starting") : t("action.openToolsTerminal")}
            </Button>
          ) : undefined
        }
      />
      <PageColumn gap="md" className="mb-8 min-h-0 flex-1">
        {tools.isPending ? (
          <LoadingRows label={t("common.loading")} rows={2} />
        ) : tools.isError ? (
          <Problem error={tools.error} />
        ) : !tools.data.available ? (
          <Empty variant="outline">
            <EmptyHeader>
              <EmptyTitle>{t("tools.localTitle")}</EmptyTitle>
              <EmptyDescription>
                {t("sentence.problem.badRequest.fix.toolsLocally")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <>
            {failed ? <Problem problem={failed} retry={start} /> : null}
            {pods.length > 0 ? (
              <ToolsPods
                project={project}
                pods={pods}
                shown={shown}
                onOpen={(name) => {
                  setAttempt(attempt + 1)
                  setOpen(name)
                }}
              />
            ) : null}
            {shown ? (
              <div className="flex min-h-105 flex-1 flex-col gap-3">
                <Session
                  key={`${shown}#${attempt}`}
                  url={toolsTerminalUrl(window.location, project, shown)}
                  project={project}
                  closed={(why) =>
                    why === "expired" ? (
                      <div className="flex flex-wrap items-center gap-3">
                        <p className="text-sm">{t("tools.expired")}</p>
                        <Button size="sm" onClick={() => setAttempt(attempt + 1)}>
                          {t("tools.reopen")}
                        </Button>
                      </div>
                    ) : null
                  }
                  onReconnect={() => setAttempt(attempt + 1)}
                />
              </div>
            ) : null}
          </>
        )}
      </PageColumn>
      {dialog}
    </div>
  )
}

function ToolsPods({
  project,
  pods,
  shown,
  onOpen,
}: {
  project: string
  pods: { name: string; ready: boolean; phase: string; stopsAt?: string | null }[]
  shown?: string
  onOpen: (name: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const stop = useMutation({
    mutationFn: async (name: string) =>
      unwrap(
        await api.DELETE("/api/v1/projects/{project}/tools/{name}", {
          params: { path: { project, name } },
        }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: toolsQuery(project).queryKey }),
  })
  return (
    <section className="flex flex-col gap-2">
      <SectionLabel>{t("tools.yours")}</SectionLabel>
      <Card>
        {pods.map((pod) => {
          const stopping = stop.isPending && stop.variables === pod.name
          return (
            <CardRow
              key={pod.name}
              label={
                stopping ? t("tools.stopping") : pod.ready ? t("tools.ready") : t("tools.starting")
              }
              description={
                pod.stopsAt ? t("tools.stopsAt", formatAgo(pod.stopsAt, locale)) : undefined
              }
            >
              <div className="flex items-center gap-2">
                {pod.ready && pod.name !== shown ? (
                  <Button size="sm" variant="outline" onClick={() => onOpen(pod.name)}>
                    {t("tools.open")}
                  </Button>
                ) : null}
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={stopping}
                  onClick={() => stop.mutate(pod.name)}
                >
                  {t("tools.stop")}
                </Button>
              </div>
            </CardRow>
          )
        })}
      </Card>
      {stop.isError ? <Problem error={stop.error} /> : null}
    </section>
  )
}

function toolsTerminalUrl(
  location: Pick<Location, "protocol" | "host">,
  project: string,
  name: string,
): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  return `${scheme}//${location.host}/api/v1/projects/${encodeURIComponent(project)}/tools/${encodeURIComponent(name)}/terminal`
}
