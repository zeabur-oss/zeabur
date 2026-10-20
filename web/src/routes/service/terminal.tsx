// Protocol: crates/zeabur-api/src/pods/exec.rs.
import { useQuery } from "@tanstack/react-query"
import { FitAddon } from "@xterm/addon-fit"
import { Terminal, type ITheme } from "@xterm/xterm"
import "@xterm/xterm/css/xterm.css"
import { useEffect, useReducer, useRef, useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"

import { sessionQuery, whoamiQuery } from "@/api/queries"
import { Problem } from "@/components/problem"
import { Refused } from "@/components/refused"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { isRefusal } from "@/model/problem"
import type { Service } from "@/model/service"
import {
  execRole,
  nextState,
  parseFrame,
  reconnectsWithRights,
  terminalUrl,
  type ClientFrame,
  type TerminalState,
} from "@/model/terminal"

export function TerminalTab({
  project,
  service,
  pod: selectedPod,
  onPodChange,
}: {
  project: string
  service: Service
  pod?: string
  onPodChange: (pod: string) => void
}) {
  const t = useT()
  const pods = service.pods
  const pod = pods.find((p) => p.name === selectedPod) ?? pods.find((p) => p.ready) ?? pods[0]
  const containerNames =
    pod && pod.containers.length > 0 ? pod.containers : service.containers.map((c) => c.name)
  const [container, setContainer] = useState<string>()
  const chosen = container && containerNames.includes(container) ? container : containerNames[0]
  const [attempt, setAttempt] = useState(0)

  if (!pod) {
    return (
      <div className="flex flex-1 items-center justify-center rounded-xl border border-dashed p-8 text-sm text-muted-foreground">
        {t("logs.noPods")}
      </div>
    )
  }
  const url = terminalUrl(window.location, project, pod.name, chosen)
  return (
    <div className="flex min-h-105 flex-1 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Select
          size="sm"
          value={pod.name}
          onValueChange={onPodChange}
          aria-label={t("terminal.pod")}
          className="max-w-full"
          items={pods.map((p) => ({ value: p.name, label: p.name }))}
        />
        {containerNames.length > 1 ? (
          <Select
            size="sm"
            value={chosen}
            onValueChange={setContainer}
            aria-label={t("logs.container")}
            items={containerNames.map((name) => ({ value: name, label: name }))}
          />
        ) : null}
      </div>
      <Session
        key={`${url}#${attempt}`}
        url={url}
        project={project}
        pod={pod.name}
        container={chosen}
        // Not `a => a + 1`: the refusal's button and the rights it granted both call this, and
        // must start one connection, not two.
        onReconnect={() => setAttempt(attempt + 1)}
      />
    </div>
  )
}

function Session({
  url,
  project,
  pod,
  container,
  onReconnect,
}: {
  url: string
  project: string
  pod: string
  container?: string
  onReconnect: () => void
}) {
  const t = useT()
  const host = useRef<HTMLDivElement>(null)
  const [state, dispatch] = useReducer(nextState, { phase: "connecting" })
  const elevation = useQuery(sessionQuery).data?.elevation
  const seen = useRef(elevation)
  // The server closes an admin terminal when the admin rights end.
  const [asAdmin] = useState(elevation === "active")

  useEffect(() => {
    const before = seen.current
    seen.current = elevation
    if (reconnectsWithRights(state, before, elevation)) onReconnect()
  }, [state, elevation, onReconnect])

  useEffect(() => {
    const element = host.current
    if (!element) return
    let disposed = false
    let teardown = () => {}
    const fontSize = 13
    const fontFamily = getComputedStyle(element).fontFamily
    // xterm.js measures a cell once; wait for JetBrains Mono so the grid fits the glyphs.
    void document.fonts
      .load(`${fontSize}px ${fontFamily}`)
      .catch(() => [])
      .then(() => {
        if (disposed) return
        teardown = connect(element, url, { fontSize, fontFamily }, dispatch)
      })
    return () => {
      disposed = true
      teardown()
    }
  }, [url])

  const live = state.phase === "connecting" || state.phase === "open"
  // Hidden, not unmounted: the screen lives in this element until the session ends.
  const screen = state.phase === "error" ? "hidden" : "flex"
  return (
    <>
      <Notice
        state={state}
        project={project}
        pod={pod}
        container={container}
        onReconnect={onReconnect}
      />
      <div
        className={cn(
          "min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-v2-border bg-card",
          screen,
        )}
      >
        <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-muted-foreground">
          <output className="inline-flex items-center gap-1.5">
            {state.phase === "open" ? (
              <span className="size-2 rounded-full bg-ok" aria-hidden="true" />
            ) : null}
            {statusText(state, asAdmin, t)}
          </output>
          <Button
            variant="ghost"
            size="xs"
            className="ml-auto"
            onClick={onReconnect}
            disabled={state.phase === "connecting"}
          >
            <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("terminal.reconnect")}
          </Button>
        </div>
        <div
          ref={host}
          role="group"
          aria-label={t("terminal.label")}
          aria-disabled={!live}
          className="min-h-0 flex-1 p-2 font-mono"
        />
      </div>
    </>
  )
}

function statusText(state: TerminalState, asAdmin: boolean, t: ReturnType<typeof useT>): string {
  switch (state.phase) {
    case "connecting":
      return t("terminal.connecting")
    case "open":
      return t("terminal.open")
    case "exited":
      return state.code === undefined ? t("terminal.ended") : t("terminal.exited", state.code)
    case "error":
      return t("terminal.ended")
    case "closed":
      return state.why === "expired" && asAdmin
        ? t("terminal.adminEnded")
        : t(`terminal.${state.why}`)
  }
}

function Notice({
  state,
  project,
  pod,
  container,
  onReconnect,
}: {
  state: TerminalState
  project: string
  pod: string
  container?: string
  onReconnect: () => void
}) {
  const t = useT()
  const problem = state.phase === "error" ? state.problem : undefined
  const exec = problem?.denied?.resource === "pods/exec"
  const whoami = useQuery({ ...whoamiQuery, enabled: exec })
  if (problem && isRefusal(problem))
    return (
      <Refused refusal={problem} what={t("refused.terminal")} retry={onReconnect}>
        {exec ? (
          <>
            <p className="text-sm text-muted-foreground">{t("terminal.rbac")}</p>
            <YamlBlock
              yaml={execRole(project, whoami.data?.username || t("access.you"))}
              label={t("terminal.roleLabel")}
            />
          </>
        ) : null}
      </Refused>
    )
  if (problem) return <Problem problem={problem} retry={onReconnect} />
  if (state.phase === "exited" && state.reason === "command-not-found") {
    const target = container ? ` --target=${container}` : ""
    return (
      <div className="flex flex-col gap-3">
        <p className="text-sm">{t("terminal.noShell")}</p>
        <YamlBlock
          yaml={`kubectl debug -it -n ${project} ${pod} --image=busybox:1.37${target}\n`}
          label={t("terminal.debugLabel")}
        />
      </div>
    )
  }
  if (state.phase === "exited" && state.reason === "failed") {
    return (
      <p className="text-sm text-muted-foreground">
        {t("terminal.failed")} {state.message ? <span translate="no">{state.message}</span> : null}
      </p>
    )
  }
  return null
}

function connect(
  element: HTMLElement,
  url: string,
  font: { fontSize: number; fontFamily: string },
  dispatch: (event: Parameters<typeof nextState>[1]) => void,
): () => void {
  const terminal = new Terminal({ ...font, cursorBlink: true, theme: themeFromTokens() })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  terminal.open(element)
  fit.fit()

  const socket = new WebSocket(url)
  socket.binaryType = "arraybuffer"
  const encoder = new TextEncoder()
  let live = false
  const send = (frame: ClientFrame) => {
    if (live) socket.send(JSON.stringify(frame))
  }
  const sendSize = () => send({ type: "resize", cols: terminal.cols, rows: terminal.rows })

  socket.addEventListener("message", (event: MessageEvent<string | ArrayBuffer>) => {
    if (typeof event.data !== "string") {
      terminal.write(new Uint8Array(event.data))
      return
    }
    const frame = parseFrame(event.data)
    if (!frame) return
    live = frame.type === "ready"
    if (live) {
      sendSize()
      terminal.focus()
    }
    dispatch({ type: "frame", frame })
  })
  socket.addEventListener("close", (event) => {
    live = false
    dispatch({ type: "close", code: event.code })
  })
  const typed = terminal.onData((data) => {
    if (live) socket.send(encoder.encode(data))
  })
  const resized = terminal.onResize(sendSize)
  const sizes = new ResizeObserver(() => fit.fit())
  sizes.observe(element)
  const themes = new MutationObserver(() => {
    terminal.options.theme = themeFromTokens()
  })
  themes.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })

  return () => {
    themes.disconnect()
    sizes.disconnect()
    typed.dispose()
    resized.dispose()
    socket.close()
    terminal.dispose()
  }
}

/** xterm.js does not parse `oklch()`: each token is painted on a one-pixel canvas and read back. */
function themeFromTokens(): ITheme {
  const canvas = document.createElement("canvas")
  canvas.width = 1
  canvas.height = 1
  const context = canvas.getContext("2d", { willReadFrequently: true })
  const styles = getComputedStyle(document.documentElement)
  const color = (token: string, alpha = 1) => {
    const value = styles.getPropertyValue(token).trim()
    if (!context || !value) return undefined
    context.clearRect(0, 0, 1, 1)
    context.fillStyle = value
    context.fillRect(0, 0, 1, 1)
    const [r = 0, g = 0, b = 0, a = 255] = context.getImageData(0, 0, 1, 1).data
    return `rgba(${r}, ${g}, ${b}, ${((a / 255) * alpha).toFixed(3)})`
  }
  const dark = document.documentElement.classList.contains("dark")
  return {
    background: color("--card"),
    foreground: color("--foreground"),
    cursor: color("--foreground"),
    cursorAccent: color("--card"),
    selectionBackground: color("--muted-foreground", 0.35),
    black: dark ? color("--muted") : color("--foreground"),
    white: dark ? color("--foreground") : color("--muted-foreground"),
    brightBlack: color("--muted-foreground"),
    brightWhite: color("--foreground"),
    red: color("--fail"),
    green: color("--ok"),
    yellow: color("--warn"),
    blue: color("--progress"),
    brightRed: color("--fail"),
    brightGreen: color("--ok"),
    brightYellow: color("--warn"),
    brightBlue: color("--progress"),
  }
}
