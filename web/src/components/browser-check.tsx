import { useEffect } from "react"

import { StatusDot } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { useT } from "@/i18n"
import { useSeen } from "@/lib/in-view"
import { store } from "@/lib/stores"
import { CHECK_EVERY, check, checkUrl, due, type BrowserCheck } from "@/model/browser-check"

const checks = store<ReadonlyMap<string, BrowserCheck>>(new Map())

const put = (url: string, result: BrowserCheck) =>
  checks.set(new Map(checks.get()).set(url, result))

function start(url: string) {
  if (checks.get().get(url)?.state === "checking") return
  put(url, { state: "checking" })
  void check(url, (u, init) => fetch(u, init), Date.now).then((result) => put(url, result))
}

/** The last result for a domain, as the lines on screen checked it. */
export function useBrowserChecks(): (address: string, tls: boolean) => BrowserCheck | undefined {
  const results = checks.useValue()
  return (address, tls) => {
    const url = checkUrl(address, tls)
    return url ? results.get(url) : undefined
  }
}

/** Whether the person's browser reaches a domain the cluster could not check: once it is on
 * screen and the tab is visible, again at most once a minute or when asked. */
export function BrowserCheckLine({ address, tls }: { address: string; tls: boolean }) {
  const t = useT()
  const [ref, seen] = useSeen<HTMLDivElement>()
  const url = checkUrl(address, tls)
  const result = checks.useValue().get(url ?? "") ?? (url ? undefined : { state: "skipped" })

  useEffect(() => {
    if (!url || !seen) return undefined
    const run = () => {
      if (document.visibilityState === "visible" && due(checks.get().get(url), Date.now()))
        start(url)
    }
    run()
    const timer = setInterval(run, CHECK_EVERY)
    document.addEventListener("visibilitychange", run)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", run)
    }
  }, [url, seen])

  const state = result?.state ?? "checking"
  return (
    <div ref={ref} className="flex flex-col gap-0.5 text-sm">
      <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="flex items-center gap-1.5 text-secondary" role="status">
          <StatusDot
            health={state === "reachable" ? "ok" : state === "unreachable" ? "fail" : "idle"}
          />
          {t(`browserCheck.${state}`)}
        </span>
        {url ? (
          <Button
            variant="ghost"
            size="xs"
            disabled={state === "checking"}
            onClick={() => start(url)}
          >
            {t("browserCheck.again")}
          </Button>
        ) : null}
      </span>
      {state === "unreachable" ? (
        <span className="type-meta text-secondary">{t("browserCheck.unreachableHint")}</span>
      ) : null}
      {state === "reachable" || state === "unreachable" ? (
        <span className="type-meta text-tertiary">{t("browserCheck.scope")}</span>
      ) : null}
    </div>
  )
}
