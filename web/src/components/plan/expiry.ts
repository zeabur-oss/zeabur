import { useEffect, useState } from "react"

/** A plan's countdown shows from this many seconds left. */
export const COUNTDOWN_FROM_SECONDS = 120

export function useSecondsLeft(expiresAt?: string): number | undefined {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])
  return expiresAt ? Math.max(0, Math.round((Date.parse(expiresAt) - now) / 1000)) : undefined
}

export function clock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`
}
