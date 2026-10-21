import { useSyncExternalStore } from "react"

/** Whether a media query matches, following the window as it resizes. */
export function useMedia(query: string): boolean {
  return useSyncExternalStore(
    (listener) => {
      const list = window.matchMedia(query)
      list.addEventListener("change", listener)
      return () => list.removeEventListener("change", listener)
    },
    () => window.matchMedia(query).matches,
  )
}

/** Below `md`: a phone, where the project page triages. */
export const PHONE = "(max-width: 47.99rem)"
/** From `2xl`: room for the map and Activity side by side. */
export const WIDE = "(min-width: 96rem)"
