import { invoke } from "@tauri-apps/api/core"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { Spinner } from "@zeabur-oss/design-system/components/spinner"
import { useEffect, useState } from "react"

import { useI18n } from "@/i18n"

/** One start per attempt: React runs an effect twice in development, and each start replaces the
 * window's service. Each resolves to the service's one-time link. */
const starts = new Map<number, Promise<string>>()
function start(attempt: number) {
  let started = starts.get(attempt)
  if (!started) {
    started = invoke<string>("start_cluster")
    starts.set(attempt, started)
  }
  return started
}

type State = { state: "starting" } | { state: "failed"; reason: string }

/** The page a cluster window shows until its local service answers (`?name=…`). The app knows
 * which context the window opens. */
export function ClusterPage() {
  const { t } = useI18n()
  const name = new URLSearchParams(window.location.search).get("name") ?? ""
  const [state, setState] = useState<State>({ state: "starting" })
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    document.title = `${name} - Zeabur`
  }, [name])

  useEffect(() => {
    let cancelled = false
    start(attempt)
      .then((url) => {
        if (!cancelled) window.location.replace(url)
      })
      .catch((error: unknown) => {
        if (!cancelled) setState({ state: "failed", reason: String(error) })
      })
    return () => {
      cancelled = true
    }
  }, [attempt])

  if (state.state === "starting") {
    return (
      <Empty className="h-screen">
        <EmptyHeader>
          <EmptyMedia>
            <Spinner />
          </EmptyMedia>
          <EmptyTitle>{t("desktop.clusterOpening", name)}</EmptyTitle>
          <EmptyDescription>{t("desktop.clusterOpeningBody")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  return (
    <Empty className="h-screen">
      <EmptyHeader>
        <EmptyMedia>
          <Illustration kind="error" />
        </EmptyMedia>
        <EmptyTitle>{t("desktop.clusterFailed", name)}</EmptyTitle>
        <EmptyDescription className="break-words select-text">{state.reason}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <div className="flex flex-wrap justify-center gap-2">
          <Button
            onClick={() => {
              setState({ state: "starting" })
              setAttempt((n) => n + 1)
            }}
          >
            {t("desktop.clusterRetry")}
          </Button>
          <Button variant="secondary" onClick={() => void invoke("back_to_chooser")}>
            {t("desktop.clusterBack")}
          </Button>
        </div>
      </EmptyContent>
    </Empty>
  )
}
