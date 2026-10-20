import { useEffect, useState } from "react"

import { localProblem, type Problem } from "@/api/client"

/** At most this many lines are kept while following; older ones are dropped. */
const maxLines = 20_000

export interface LogStream {
  lines: string[]
  problem?: Problem
  ended: boolean
}

function isProblem(value: unknown): value is Problem {
  return typeof value === "object" && value !== null && "detail" in value && "fix" in value
}

/**
 * Follows a log over Server-Sent Events. Lines arriving within one animation frame are added
 * together. The server's `error` event carries a problem and ends the stream; a dropped connection
 * also ends it (the browser would otherwise reconnect and repeat the tail). The caller mounts the
 * component using this hook under a `key` of the URL, so a new URL starts from an empty state.
 */
export function useLogStream(url: string): LogStream {
  const [state, setState] = useState<LogStream>({ lines: [], ended: false })

  useEffect(() => {
    const source = new EventSource(url, { withCredentials: true })
    let pending: string[] = []
    let frame = 0
    let opened = false
    const flush = () => {
      frame = 0
      if (pending.length === 0) return
      const batch = pending
      pending = []
      setState((previous) => {
        const lines = previous.lines.concat(batch)
        return {
          ...previous,
          lines: lines.length > maxLines ? lines.slice(lines.length - maxLines) : lines,
        }
      })
    }
    source.addEventListener("open", () => {
      opened = true
    })
    source.addEventListener("message", (event) => {
      pending.push(event.data)
      if (!frame) frame = requestAnimationFrame(flush)
    })
    source.addEventListener("error", (event) => {
      source.close()
      let problem: Problem | undefined
      if (event instanceof MessageEvent && typeof event.data === "string") {
        try {
          const parsed: unknown = JSON.parse(event.data)
          problem = isProblem(parsed) ? parsed : localProblem(0, event.data)
        } catch {
          problem = localProblem(0, event.data)
        }
      } else if (!opened) {
        // A refused request carries no body EventSource can read; the snapshot shows the answer.
        problem = {
          ...localProblem(0, "The log stream closed before it opened."),
          summary: { key: "page.followFailed" },
          fix: "Turn off Follow; the snapshot shows the cluster's answer.",
          fixKey: { key: "page.followOff" },
        }
      }
      if (frame) cancelAnimationFrame(frame)
      flush()
      setState((previous) => ({ ...previous, problem, ended: true }))
    })
    return () => {
      if (frame) cancelAnimationFrame(frame)
      source.close()
    }
  }, [url])

  return state
}
