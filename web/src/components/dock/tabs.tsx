import { useMutation, useQuery } from "@tanstack/react-query"
import { useEffect, useRef, useState } from "react"

import { api, isStatus, unwrap } from "@/api/client"
import { clusterToolsQuery, deleteClusterTools } from "@/api/cluster-tools"
import { databasesQuery, debugQuery, projectQuery, queryClient, toolsQuery } from "@/api/queries"
import { ClusterToolsDialog } from "@/components/cluster-tools"
import { Session } from "@/components/dock/session"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { useI18n, useT } from "@/i18n"
import { openClusterKubectl, openKubectl, updateShell, type DockTab } from "@/lib/dock"
import { formatAgo } from "@/lib/format"
import { serverContainer, shellCommand, shellPod } from "@/model/databases"
import type { Pod } from "@/model/service"
import { terminalUrl, toolsTerminalUrl } from "@/model/terminal"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Select } from "@zeabur/design-system/components/select"

type ShellTabProps = { tab: Extract<DockTab, { kind: "shell" }>; shown: boolean; focus: number }

/** A shell in one of the service's containers, or a debug container beside it; for a database,
 * its engine's shell in the server's container. */
export function ShellTab(props: ShellTabProps) {
  return props.tab.database ? <DatabaseShell {...props} /> : <ServiceShell {...props} />
}

function ServiceShell({ tab, shown, focus }: ShellTabProps) {
  const t = useT()
  const view = useQuery(projectQuery(tab.project, { poll: false }))
  const service = view.data?.services.find(
    (s) => s.kind === tab.service.kind && s.name === tab.service.name,
  )
  if (view.isPending) return <LoadingRows label={t("common.loading")} rows={2} />
  if (view.isError) return <Problem error={view.error} />
  if (!service) return <NoPod gone />
  return (
    <PodShell
      tab={tab}
      shown={shown}
      focus={focus}
      pods={service.pods}
      containers={service.containers.map((c) => c.name)}
      debug
    />
  )
}

function DatabaseShell({ tab, shown, focus }: ShellTabProps) {
  const t = useT()
  const query = useQuery(databasesQuery(tab.project))
  const database = query.data?.items.find(
    (d) => d.engine === tab.database?.engine && d.name === tab.database.name,
  )
  if (query.isPending) return <LoadingRows label={t("common.loading")} rows={2} />
  if (query.isError) return <Problem error={query.error} />
  if (!database) return <NoPod gone />
  const server = serverContainer[database.engine]
  return (
    <PodShell
      tab={tab}
      shown={shown}
      focus={focus}
      pods={database.pods}
      containers={[server]}
      preferred={{ pod: shellPod(database), container: server, command: shellCommand(database) }}
    />
  )
}

function NoPod({ gone = false }: { gone?: boolean }) {
  const t = useT()
  return (
    <Empty className="flex-1">
      <EmptyHeader>
        <EmptyTitle>{gone ? t("dock.gone") : t("logs.noPods")}</EmptyTitle>
        {gone ? null : <EmptyDescription>{t("terminal.noPodsBody")}</EmptyDescription>}
      </EmptyHeader>
    </Empty>
  )
}

function PodShell({
  tab,
  shown,
  focus,
  pods,
  containers,
  preferred,
  debug: offersDebug = false,
}: ShellTabProps & {
  pods: Pod[]
  /** The containers a pod runs, when the pod does not list them. */
  containers: string[]
  /** Where it opens first, and what runs in that container instead of a shell. */
  preferred?: { pod: string; container: string; command: string[] }
  /** Offers a debug container when the image has no shell. */
  debug?: boolean
}) {
  const t = useT()
  const pod =
    pods.find((p) => p.name === tab.pod) ??
    pods.find((p) => p.name === preferred?.pod) ??
    pods.find((p) => p.ready) ??
    pods[0]
  const containerNames = pod && pod.containers.length > 0 ? pod.containers : containers
  const chosen =
    tab.container && containerNames.includes(tab.container)
      ? tab.container
      : preferred && containerNames.includes(preferred.container)
        ? preferred.container
        : containerNames[0]
  const several = containerNames.length > 1
  const [attempt, setAttempt] = useState(0)
  const [debug, setDebug] = useState<{ pod: string; target?: string; name: string }>()

  // The replica it opened in stays its own, whatever the list says next.
  useEffect(() => {
    if (pod && (tab.pod !== pod.name || (several && tab.container !== chosen)))
      updateShell(tab.id, { pod: pod.name, container: several ? chosen : undefined })
  }, [tab.id, tab.pod, tab.container, pod, chosen, several])

  if (!pod) return <NoPod />
  const debugName = debug?.pod === pod.name && debug.target === chosen ? debug.name : undefined
  const command = preferred && chosen === preferred.container ? preferred.command : []
  const url = terminalUrl(window.location, tab.project, pod.name, debugName ?? chosen, command)
  return (
    <Session
      id={tab.id}
      url={url}
      attempt={attempt}
      project={tab.project}
      shown={shown}
      focus={focus}
      debug={debugName !== undefined}
      toolbar={
        <>
          {pods.length > 1 ? (
            <Select
              size="sm"
              value={pod.name}
              onValueChange={(name) => updateShell(tab.id, { pod: name })}
              aria-label={t("terminal.pod")}
              className="max-w-60"
              items={pods.map((p) => ({ value: p.name, label: p.name }))}
            />
          ) : (
            <span className="truncate font-mono" translate="no">
              {pod.name}
            </span>
          )}
          {several ? (
            <Select
              size="sm"
              value={chosen}
              onValueChange={(container) => updateShell(tab.id, { container })}
              aria-label={t("logs.container")}
              items={containerNames.map((name) => ({ value: name, label: name }))}
            />
          ) : null}
        </>
      }
      noShell={
        offersDebug ? (
          <DebugOffer
            project={tab.project}
            pod={pod.name}
            container={chosen}
            onAttach={(name) => setDebug({ pod: pod.name, target: chosen, name })}
          />
        ) : undefined
      }
      // Not `a => a + 1`: the refusal's button and the rights it granted both call this, and
      // must start one connection, not two.
      onReconnect={() => setAttempt(attempt + 1)}
    />
  )
}

/** No shell in the image: a debug container brings one. Planned and reviewed as any change; a
 * running one is used again without asking. */
function DebugOffer({
  project,
  pod,
  container,
  onAttach,
}: {
  project: string
  pod: string
  container?: string
  onAttach: (name: string) => void
}) {
  const t = useT()
  const [waiting, setWaiting] = useState(false)
  const status = useQuery({
    ...debugQuery(project, pod, container),
    refetchInterval: (query) => {
      const data = query.state.data
      return waiting && !data?.problem && data?.container?.state !== "running" ? 1500 : false
    },
  })
  const { plan, dialog } = usePlanner(project, () => setWaiting(true), true)
  const current = status.data?.container ?? undefined
  const problem = waiting ? (status.data?.problem ?? undefined) : undefined
  const running = current?.state === "running" ? current.name : undefined
  useEffect(() => {
    if (waiting && running) onAttach(running)
  }, [waiting, running, onAttach])
  const ended = waiting && current?.state === "terminated"
  const start = () => {
    if (running) {
      onAttach(running)
      return
    }
    setWaiting(false)
    plan({
      title: t("action.openDebugTerminal"),
      changes: [{ op: "debugContainer", pod, container }],
      quiet: true,
    })
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col items-start gap-3 overflow-y-auto px-3 pb-3">
      <p className="max-w-prose text-sm">
        {ended ? t("terminal.debugEnded") : t("terminal.noShell")}
      </p>
      {problem ? <Problem problem={problem} retry={start} /> : null}
      {waiting && !problem && !ended ? (
        <output className="text-sm text-secondary">{t("terminal.debugStarting")}</output>
      ) : (
        <Button size="sm" onClick={start}>
          {t("action.openDebugTerminal")}
        </Button>
      )}
      {dialog}
    </div>
  )
}

type KubectlTabProps = { tab: Extract<DockTab, { kind: "kubectl" }>; shown: boolean; focus: number }

const toolsOf = (project: string | undefined) =>
  project === undefined ? clusterToolsQuery : toolsQuery(project)

/** Plans a tools pod: in a project as any change, for the cluster in a plan of its own. */
function useStartTools(project: string | undefined, onApplied: () => void) {
  const t = useT()
  const planner = usePlanner(project ?? "", onApplied, true)
  const [cluster, setCluster] = useState(false)
  if (project !== undefined)
    return {
      start: () =>
        planner.plan({
          title: t("action.openToolsTerminal"),
          changes: [{ op: "clusterTools" }],
          quiet: true,
        }),
      dialog: planner.dialog,
    }
  return {
    start: () => setCluster(true),
    dialog: cluster ? (
      <ClusterToolsDialog onClose={() => setCluster(false)} onApplied={onApplied} />
    ) : null,
  }
}

/** kubectl and helm in a tools pod of the person's, in a project or for the whole cluster: planned
 * the first time, used again after. */
export function KubectlTab({ tab, shown, focus }: KubectlTabProps) {
  const t = useT()
  const project = tab.project
  const [waiting, setWaiting] = useState(false)
  const tools = useQuery({
    ...toolsOf(project),
    refetchInterval: (query) =>
      waiting || query.state.data?.pods.some((p) => !p.ready) ? 1500 : 15_000,
  })
  const [open, setOpen] = useState<string>()
  const [attempt, setAttempt] = useState(0)
  const { start, dialog } = useStartTools(project, () => setWaiting(true))
  const pods = tools.data?.pods ?? []
  const ready = pods.find((p) => p.ready)?.name
  const shownPod = pods.some((p) => p.name === open) ? open : undefined
  const failed = waiting ? pods.find((p) => p.problem)?.problem : undefined

  const attach = (name: string) => {
    setAttempt(attempt + 1)
    setOpen(name)
  }

  useEffect(() => {
    if (!waiting || !ready) return
    setWaiting(false)
    setOpen(ready)
  }, [waiting, ready])

  // Each ask from outside the tab, once the list is known.
  const answered = useRef(0)
  const data = tools.data
  useEffect(() => {
    if (!data || answered.current === tab.asked) return
    answered.current = tab.asked
    if (!data.available || waiting) return
    const named = data.pods.find((p) => p.name === tab.pod && p.ready)?.name
    if (named) setOpen(named)
    else if (!shownPod && ready) setOpen(ready)
    else if (!shownPod) start()
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- answers the ask, once per ask
  }, [tab.asked, data])

  if (tools.isPending) return <LoadingRows label={t("common.loading")} rows={2} />
  if (tools.isError)
    return (
      <Problem
        error={tools.error}
        what={project === undefined ? t("clusterTools.what") : undefined}
      />
    )
  if (!tools.data.available)
    return (
      <Empty className="flex-1">
        <EmptyHeader>
          <EmptyTitle>{t("tools.localTitle")}</EmptyTitle>
          <EmptyDescription>{t("sentence.problem.badRequest.fix.toolsLocally")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  if (shownPod)
    return (
      <Session
        id={tab.id}
        url={toolsTerminalUrl(window.location, project, shownPod)}
        attempt={attempt}
        project={project ?? tools.data.namespace}
        shown={shown}
        focus={focus}
        grant={project !== undefined}
        onReconnect={() => setAttempt(attempt + 1)}
      />
    )
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-0 max-w-prose flex-1 text-sm text-secondary">
          {project === undefined ? t("clusterTools.about") : t("tools.about")}
        </p>
        <Button size="sm" onClick={start} disabled={waiting}>
          {waiting ? t("tools.starting") : t("action.openToolsTerminal")}
        </Button>
      </div>
      {failed ? <Problem problem={failed} retry={start} /> : null}
      {pods.length > 0 ? <ToolsPods project={project} pods={pods} onOpen={attach} /> : null}
      {dialog}
    </div>
  )
}

/** The person's running tools pods in a project, or the cluster's, to use again or stop. */
export function ToolsPods({
  project,
  pods,
  onOpen,
}: {
  project: string | undefined
  pods: { name: string; ready: boolean; stopsAt?: string | null }[]
  onOpen?: (name: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const stop = useMutation({
    mutationFn: async (name: string) => {
      if (project === undefined) return deleteClusterTools(name)
      try {
        unwrap(
          await api.DELETE("/api/v1/projects/{project}/tools/{name}", {
            params: { path: { project, name } },
          }),
        )
      } catch (error) {
        // Already gone: deleted once its terminals closed, or at its deadline.
        if (!isStatus(error, 404)) throw error
      }
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: toolsOf(project).queryKey }),
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
                {pod.ready ? (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      onOpen
                        ? onOpen(pod.name)
                        : project === undefined
                          ? openClusterKubectl({ pod: pod.name })
                          : openKubectl(project, { pod: pod.name })
                    }
                  >
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
