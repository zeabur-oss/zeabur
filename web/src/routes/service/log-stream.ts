import { useEffect, useState } from "react"
import * as z from "zod/mini"

import { localProblem, type Problem } from "@/api/client"

const maxLines = 20_000

export interface LogStream {
  lines: string[]
  problem?: Problem
  /** The connection is open; until then nothing is followed yet. */
  open: boolean
  ended: boolean
  /** When the stream ended, RFC 3339. */
  endedAt?: string
}

// The schema checks only `detail` and `fix`. The server defines the rest of `Problem`.
const streamProblem = z.looseObject({ detail: z.string(), fix: z.string() })

/** A dropped connection ends the stream: EventSource would reconnect and repeat the tail. Callers
 * key the component by `url`, so a new URL starts empty. */
export function useLogStream(url: string): LogStream {
  const [state, setState] = useState<LogStream>({ lines: [], open: false, ended: false })

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
      setState((previous) => ({ ...previous, open: true }))
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
          const parsed = streamProblem.safeParse(JSON.parse(event.data))
          problem = parsed.success ? (parsed.data as Problem) : localProblem(0, event.data)
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
      setState((previous) => ({
        ...previous,
        problem,
        open: false,
        ended: true,
        endedAt: new Date().toISOString(),
      }))
    })
    return () => {
      if (frame) cancelAnimationFrame(frame)
      source.close()
    }
  }, [url])

  return state
}
