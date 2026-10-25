// Protocol: crates/zeabur-api/src/pods/exec.rs and crates/zeabur-api/src/tools.rs.
import { useQuery } from "@tanstack/react-query"
import { FitAddon } from "@xterm/addon-fit"
import { Terminal, type ITheme } from "@xterm/xterm"
import "@xterm/xterm/css/xterm.css"
import { useEffect, useReducer, useRef, useState, type ReactNode } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"

import { whoamiQuery } from "@/api/queries"
import { Problem } from "@/components/problem"
import { Refused } from "@/components/refused"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { isRefusal } from "@/model/problem"
import {
  execRole,
  nextState,
  parseFrame,
  type ClientFrame,
  type TerminalEvent,
  type TerminalState,
} from "@/model/terminal"
import { Button } from "@zeabur/design-system/components/button"

/**
 * One terminal: its screen lives as long as the tab, and a reconnect opens a new connection on the
 * same screen, so what was printed stays.
 */
export function Session({
  url,
  attempt,
  project,
  shown,
  focus,
  debug = false,
  toolbar,
  noShell,
  ended,
  grant = true,
  onReconnect,
}: {
  url: string
  /** A new number reconnects. */
  attempt: number
  project: string
  /** Its tab is the one shown and the dock is open. */
  shown: boolean
  /** Bumped when the shown terminal should take the keyboard. */
  focus: number
  debug?: boolean
  toolbar?: ReactNode
  noShell?: ReactNode
  /** Shown once the shell has ended or closed, in place of reconnecting. */
  ended?: ReactNode
  /** A refusal to exec offers a Role that grants it. */
  grant?: boolean
  onReconnect: () => void
}) {
  const t = useT()
  const host = useRef<HTMLDivElement>(null)
  const [terminal, setTerminal] = useState<Terminal>()
  const [state, dispatch] = useReducer(nextState, { phase: "connecting" })
  const visible = useRef(shown)
  visible.current = shown

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
        const [created, dispose] = screen(element, { fontSize, fontFamily })
        teardown = dispose
        setTerminal(created)
      })
    return () => {
      disposed = true
      teardown()
    }
  }, [])

  useEffect(() => {
    if (!terminal) return
    dispatch({ type: "connect" })
    return relay(terminal, url, (event) => {
      dispatch(event)
      if (event.type === "frame" && event.frame.type === "ready" && visible.current)
        terminal.focus()
    })
  }, [terminal, url, attempt])

  useEffect(() => {
    if (!shown) return
    const frame = requestAnimationFrame(() => {
      if (terminal) terminal.focus()
      else host.current?.focus()
    })
    return () => cancelAnimationFrame(frame)
  }, [shown, focus, terminal])

  const live = state.phase === "connecting" || state.phase === "open"
  const noShellShown =
    noShell !== undefined && state.phase === "exited" && state.reason === "command-not-found"
  // Hidden, not unmounted: the screen lives in this element until the tab closes.
  const hidden = state.phase === "error" || noShellShown
  const over =
    ended !== undefined &&
    (state.phase === "closed" || (state.phase === "exited" && state.reason !== "command-not-found"))
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-control-lg shrink-0 flex-wrap items-center gap-2 px-3 py-1.5 text-xs text-secondary">
        {toolbar}
        <output className="inline-flex items-center gap-1.5">
          {state.phase === "open" ? (
            <span className="size-2 rounded-full bg-ok" aria-hidden="true" />
          ) : null}
          {state.phase === "open" && debug ? t("terminal.debugOpen") : statusText(state, t)}
        </output>
        {over ? null : (
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
        )}
      </div>
      {over ? (
        ended
      ) : (
        <Notice
          state={state}
          project={project}
          noShell={noShell}
          grant={grant}
          onReconnect={onReconnect}
        />
      )}
      <div
        ref={host}
        role="group"
        tabIndex={-1}
        aria-label={t("terminal.label")}
        aria-disabled={!live}
        className={cn("min-h-0 flex-1 px-3 pb-2 font-mono outline-none", hidden && "hidden")}
      />
    </div>
  )
}

function statusText(state: TerminalState, t: ReturnType<typeof useT>): string {
  switch (state.phase) {
    case "connecting":
      return t("terminal.connecting")
    case "open":
      return t("terminal.open")
    case "exited":
      return state.code === undefined ? t("terminal.ended") : t("terminal.exited", state.code)
    case "error":
    case "closed":
      return t("terminal.ended")
  }
}

function Notice({
  state,
  project,
  noShell,
  grant,
  onReconnect,
}: {
  state: TerminalState
  project: string
  noShell?: ReactNode
  grant: boolean
  onReconnect: () => void
}) {
  const t = useT()
  const problem = state.phase === "error" ? state.problem : undefined
  const exec = grant && problem?.denied?.resource === "pods/exec"
  const whoami = useQuery({ ...whoamiQuery, enabled: exec })
  const box = (child: ReactNode) => (
    <div className="flex max-h-full shrink-0 flex-col gap-3 overflow-y-auto px-3 pb-3">{child}</div>
  )
  if (problem && isRefusal(problem))
    return box(
      <Refused refusal={problem} what={t("refused.terminal")}>
        {exec ? (
          <>
            <p className="text-sm text-secondary">{t("terminal.rbac")}</p>
            <YamlBlock
              yaml={execRole(project, whoami.data?.username || t("access.you"))}
              label={t("terminal.roleLabel")}
            />
          </>
        ) : null}
      </Refused>,
    )
  if (problem) return box(<Problem problem={problem} />)
  if (state.phase === "exited" && state.reason === "command-not-found") return noShell ?? null
  if (state.phase === "exited" && state.reason === "failed")
    return box(
      <p className="text-sm text-secondary">
        {t("terminal.failed")} {state.message ? <span translate="no">{state.message}</span> : null}
      </p>,
    )
  if (state.phase === "closed")
    return (
      <div
        role="status"
        className="mx-3 mb-2 flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-main bg-muted px-3 py-2"
      >
        <p className="min-w-0 flex-1 text-sm">{t(`terminal.${state.why}`)}</p>
        <Button size="sm" onClick={onReconnect}>
          {t("tools.reopen")}
        </Button>
      </div>
    )
  return null
}

/** The screen, sized to its element while it has a size: a hidden tab keeps its columns. */
function screen(
  element: HTMLElement,
  font: { fontSize: number; fontFamily: string },
): [Terminal, () => void] {
  const terminal = new Terminal({ ...font, cursorBlink: true, theme: themeFromTokens() })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  terminal.open(element)
  const refit = () => {
    if (element.clientWidth > 0 && element.clientHeight > 0) fit.fit()
  }
  refit()
  const sizes = new ResizeObserver(refit)
  sizes.observe(element)
  const themes = new MutationObserver(() => {
    terminal.options.theme = themeFromTokens()
  })
  themes.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
  return [
    terminal,
    () => {
      themes.disconnect()
      sizes.disconnect()
      terminal.dispose()
    },
  ]
}

/** One connection between the screen and the server, until it closes or is replaced. */
function relay(terminal: Terminal, url: string, dispatch: (event: TerminalEvent) => void) {
  const socket = new WebSocket(url)
  socket.binaryType = "arraybuffer"
  const encoder = new TextEncoder()
  let live = false
  let replaced = false
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
    if (live) sendSize()
    dispatch({ type: "frame", frame })
  })
  socket.addEventListener("close", (event) => {
    live = false
    if (!replaced) dispatch({ type: "close", code: event.code })
  })
  const typed = terminal.onData((data) => {
    if (live) socket.send(encoder.encode(data))
  })
  const resized = terminal.onResize(sendSize)
  // A reconnect starts on a fresh line under what the last connection printed.
  if (terminal.buffer.active.cursorX > 0) terminal.write("\r\n")
  return () => {
    replaced = true
    typed.dispose()
    resized.dispose()
    socket.close()
  }
}

/** xterm.js parses neither `oklch()` nor `light-dark()`: each token is resolved on an element of the
 * page, then painted on a one-pixel canvas and read back. */
function themeFromTokens(): ITheme {
  const canvas = document.createElement("canvas")
  canvas.width = 1
  canvas.height = 1
  const context = canvas.getContext("2d", { willReadFrequently: true })
  const probe = document.createElement("span")
  probe.hidden = true
  document.body.append(probe)
  const color = (token: string, alpha = 1) => {
    probe.style.color = ""
    probe.style.color = `var(${token})`
    const value = getComputedStyle(probe).color
    if (!context || !value) return undefined
    context.clearRect(0, 0, 1, 1)
    context.fillStyle = value
    context.fillRect(0, 0, 1, 1)
    const [r = 0, g = 0, b = 0, a = 255] = context.getImageData(0, 0, 1, 1).data
    return `rgba(${r}, ${g}, ${b}, ${((a / 255) * alpha).toFixed(3)})`
  }
  const dark = document.documentElement.classList.contains("dark")
  const theme: ITheme = {
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
  probe.remove()
  return theme
}
