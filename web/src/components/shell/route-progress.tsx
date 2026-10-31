import { useRouterState } from "@tanstack/react-router"
import { useEffect, useState } from "react"

import { useT } from "@/i18n"

/** A click shows its answer within this long. A page that arrives sooner never shows the bar. */
const SHOW_AFTER_MS = 100

/** A slim bar at the top of the panel while a navigation waits for its page's data. The old page
 * stays readable; the route's skeleton still comes only after the router's own delay. */
export function RouteProgress() {
  const t = useT()
  const pending = useRouterState({ select: (state) => state.status === "pending" })
  const [shown, setShown] = useState(false)
  useEffect(() => {
    if (!pending) {
      setShown(false)
      return
    }
    const timer = setTimeout(() => setShown(true), SHOW_AFTER_MS)
    return () => clearTimeout(timer)
  }, [pending])
  if (!shown) return null
  return (
    <div className="relative z-20 h-0" data-slot="route-progress">
      <div
        role="progressbar"
        aria-label={t("shell.opening")}
        className="absolute inset-x-0 top-0 h-0.5 overflow-hidden bg-progress-soft"
      >
        <div className="h-full w-full bg-progress motion-safe:w-1/3 motion-safe:animate-route-progress" />
      </div>
    </div>
  )
}
