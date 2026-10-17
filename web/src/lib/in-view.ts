// Whether an element has come near the viewport, once: for sections that load only when the
// person scrolls to them. A ref callback with a cleanup, so no effect is needed.
import { useCallback, useRef, useState } from "react"

export function useSeen<T extends Element>(): [
  (node: T | null) => (() => void) | undefined,
  boolean,
] {
  const [seen, setSeen] = useState(false)
  const ref = useCallback(
    (node: T | null) => {
      if (!node || seen) return undefined
      if (typeof IntersectionObserver === "undefined") {
        setSeen(true)
        return undefined
      }
      const observer = new IntersectionObserver(
        (entries) => {
          if (entries.some((entry) => entry.isIntersecting)) {
            setSeen(true)
            observer.disconnect()
          }
        },
        { rootMargin: "200px" },
      )
      observer.observe(node)
      return () => observer.disconnect()
    },
    [seen],
  )
  return [ref, seen]
}

/** A ref callback that scrolls its element to the middle of its scroller the first time it
 * mounts (a highlighted key), and never again, so typing in it does not jump the view. */
export function useScrollOnce<T extends Element>(): (node: T | null) => void {
  const done = useRef(false)
  return useCallback((node: T | null) => {
    if (!node || done.current) return
    done.current = true
    node.scrollIntoView({ block: "center" })
  }, [])
}
