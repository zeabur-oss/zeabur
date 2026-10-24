// The cluster (or Zeabur) does not answer: one banner on top with the cause, the cluster's name,
// when it last answered and the next try; the sections under it stay quiet about it.
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query"
import { useCallback, useEffect, useRef, useSyncExternalStore } from "react"
import PlugsIcon from "~icons/ph/plugs"

import { isUnreachable, toProblem } from "@/api/client"
import { clusterQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { useI18n, useT } from "@/i18n"
import { lastKnown, rememberKnown } from "@/lib/last-known"
import { problemText } from "@/model/problem"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"

const RETRY = 15_000

// How the last read of the cluster ended. A query that failed without data goes back to pending
// on every new try, which would drop the banner each time; this keeps it until an answer comes.
let settled: unknown = null
const listeners = new Set<() => void>()
let watched: QueryClient | undefined

const readSettled = () => settled

function watch(queryClient: QueryClient) {
  if (watched === queryClient) return
  watched = queryClient
  settled = queryClient.getQueryState(clusterQuery.queryKey)?.error ?? null
  queryClient.getQueryCache().subscribe((event) => {
    if (event.type !== "updated" || event.query.queryHash !== JSON.stringify(clusterQuery.queryKey))
      return
    if (event.action.type === "error") settled = event.action.error
    else if (event.action.type === "success") settled = null
    else return
    for (const listener of listeners) listener()
  })
}

/** The cluster's state as the banner tells it; `down` while it does not answer. */
export function useReachability() {
  const cluster = useQuery({
    ...clusterQuery,
    refetchInterval: (query) => (isUnreachable(settled ?? query.state.error) ? RETRY : false),
  })
  const queryClient = useQueryClient()
  const subscribe = useCallback(
    (listener: () => void) => {
      watch(queryClient)
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
    [queryClient],
  )
  const error = useSyncExternalStore(subscribe, readSettled, readSettled)
  const down = isUnreachable(error)
  const wasDown = useRef(false)
  useEffect(() => {
    if (cluster.data) rememberKnown({ cluster: cluster.data.name, at: cluster.dataUpdatedAt })
  }, [cluster.data, cluster.dataUpdatedAt])
  useEffect(() => {
    // Back: read again what failed while it was away.
    if (wasDown.current && !down) void queryClient.invalidateQueries()
    wasDown.current = down
  }, [down, queryClient])
  const known = lastKnown()
  return {
    down,
    error,
    name: cluster.data?.name ?? known.cluster,
    lastHeard: cluster.dataUpdatedAt || known.at,
    retrying: cluster.isFetching,
    retry: () => void queryClient.invalidateQueries(),
  }
}

export function UnreachableBanner() {
  const t = useT()
  const { locale } = useI18n()
  const state = useReachability()
  if (!state.down) return null
  const problem = toProblem(state.error)
  const local = problem.type === "about:blank"
  const text = problemText(t, locale, problem)
  const cause = text.why ?? problem.detail
  return (
    <Alert variant="problem">
      <PlugsIcon aria-hidden="true" />
      <AlertTitle>
        {local
          ? t("unreachable.zeabur")
          : state.name
            ? t("unreachable.title", state.name)
            : t("problem.title.unavailable")}
      </AlertTitle>
      <AlertDescription>
        {cause ? (
          <p className="type-mono break-all" lang="en" translate="no">
            {cause}
          </p>
        ) : null}
        <p>
          {state.lastHeard ? (
            <>
              {t("unreachable.lastHeard")} <Ago iso={new Date(state.lastHeard).toISOString()} />
              {t("common.sep")}
            </>
          ) : null}
          {state.retrying ? t("unreachable.retrying") : t("unreachable.every")}
          {t("common.gap")}
          {local ? text.fix : t("problem.fix.unavailable")}
        </p>
      </AlertDescription>
      <AlertAction>
        <Button variant="outline" size="sm" onClick={state.retry} disabled={state.retrying}>
          {t("common.retry")}
        </Button>
      </AlertAction>
    </Alert>
  )
}

/** In place of a section the cluster did not answer for; the banner on top says why. */
export function NotRead() {
  const t = useT()
  return <p className="type-meta text-muted-foreground">{t("cluster.notRead")}</p>
}
