import { useEffect, useState, useSyncExternalStore } from "react"

function subscribe(onChange: () => void) {
  document.addEventListener("visibilitychange", onChange)
  return () => document.removeEventListener("visibilitychange", onChange)
}

/** Whether the tab is on screen; work that only draws should stop while it is not. */
export function usePageVisible(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => document.visibilityState === "visible",
    () => true,
  )
}

/** The time, renewed every `every` ms while the tab is visible and when it becomes visible again.
 * `enabled` false stops the timer. */
export function useNow(every: number, enabled = true): number {
  const visible = usePageVisible()
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (!enabled || !visible) return undefined
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), every)
    return () => window.clearInterval(timer)
  }, [every, enabled, visible])
  return now
}
