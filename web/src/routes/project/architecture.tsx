import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
  type ReactNode,
} from "react"
import CornersOutIcon from "~icons/ph/corners-out"
import GitBranchIcon from "~icons/ph/git-branch"
import HardDriveIcon from "~icons/ph/hard-drive"
import MinusIcon from "~icons/ph/minus"
import PlusIcon from "~icons/ph/plus"

import { Hint } from "@/components/bits"
import { Badge } from "@zeabur/design-system/components/badge"
import { usePresence } from "@zeabur/design-system/components/presence"
import { ServiceIcon } from "@/components/service-icon"
import { StatusDot, useStatusWord } from "@/components/status"
import { useT } from "@/i18n"
import { canvasViewport, type Viewport } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { edgesOf } from "@/model/connections"
import { edgeText } from "@/model/evidence"
import { originOf } from "@/model/origin"
import { bounds, fittedZoom, layout, type LaidEdge, type LaidNode, type Lane } from "@/model/layout"
import {
  claimOf,
  domainsOf,
  imageTag,
  serviceId,
  statusOf,
  volumesOf,
  type Project,
  type Service,
} from "@/model/service"
import { useProjectEvents } from "@/routes/project/data"
import { QuickLook } from "@/routes/project/quick-look"

const px = (n: number) => `${n}px`

/** The quick look's width from `sm` (`sm:w-100`); below it, it covers the whole row. */
const quickLookWidth = 400

/** Where the quick look starts, in client pixels, from `sm`. */
function quickLookLeft(canvas: HTMLElement) {
  const row = canvas.closest("[data-map-row]")
  return row && window.innerWidth >= 640
    ? row.getBoundingClientRect().right - quickLookWidth
    : undefined
}

/**
 * The services drawn by role, with what connects them, and `children` (a side column) beside them;
 * a click opens the quick look at the row's right edge, over the side column.
 */
export function ProjectMap({
  project,
  data,
  selected,
  onSelect,
  children,
}: {
  project: string
  data: Project
  selected?: string
  onSelect: (id?: string) => void
  children?: ReactNode
}) {
  const t = useT()
  const events = useProjectEvents(project)
  const edges = edgesOf(data)
  // The last service shown stays in the panel while it closes.
  const [shown, setShown] = useState(selected)
  if (selected && selected !== shown) setShown(selected)
  const open = data.services.find((s) => serviceId(s) === shown)
  const presence = usePresence(Boolean(selected && open))

  return (
    <div data-map-row className="relative flex min-h-140 flex-1 border-t border-main">
      <div className="relative min-h-0 min-w-0 flex-1 overflow-hidden">
        <Graph project={project} data={data} selected={selected} onOpen={onSelect} />
      </div>
      {children}
      {open && presence.mounted ? (
        <QuickLook
          key={serviceId(open)}
          presence={presence.props}
          project={project}
          service={open}
          services={data.services}
          edges={edges}
          events={events}
          onClose={() => onSelect(undefined)}
          onSelect={onSelect}
        />
      ) : null}
      <span className="sr-only">{t("architecture.edges", edges.length)}</span>
    </div>
  )
}

const laneTitle: Record<Lane, `lane.${Lane}`> = {
  web: "lane.web",
  internal: "lane.internal",
  workers: "lane.workers",
  data: "lane.data",
}

function Graph({
  project,
  data,
  selected,
  onOpen,
}: {
  project: string
  data: Project
  selected?: string
  onOpen: (id: string) => void
}) {
  const t = useT()
  const word = useStatusWord()
  const graph = layout(data.services, edgesOf(data))
  const box = bounds(graph)
  const viewport = useRef<HTMLDivElement>(null)
  const [view, setViewState] = useState<Viewport>(
    () => canvasViewport.get(project) ?? { x: 0, y: 0, k: 1 },
  )
  const [hoverEdge, setHoverEdge] = useState<string>()
  const [hoverNode, setHoverNode] = useState<string>()
  const drag = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null)
  const [dragging, setDragging] = useState(false)

  const setView = (next: Viewport | ((v: Viewport) => Viewport)) =>
    setViewState((previous) => {
      const value = typeof next === "function" ? next(previous) : next
      canvasViewport.set(project, value)
      return value
    })

  /** How much of the canvas's right side the open quick look hides. */
  const covered = (el: HTMLElement) => {
    const left = selected ? quickLookLeft(el) : undefined
    return left === undefined ? 0 : Math.max(0, el.getBoundingClientRect().right - left)
  }

  const fit = () => {
    const el = viewport.current
    if (!el) return
    const w = el.clientWidth - covered(el)
    const h = el.clientHeight
    const bw = box.maxX - box.minX
    const bh = box.maxY - box.minY
    const k = fittedZoom(graph, w, h)
    const node = graph.nodes.find((n) => n.id === selected)
    if (node && k < 0.75) {
      const z = 0.75
      setView({ k: z, x: w / 2 - (node.x + node.w / 2) * z, y: h / 2 - (node.y + node.h / 2) * z })
      return
    }
    setView({ k, x: (w - bw * k) / 2 - box.minX * k, y: (h - bh * k) / 2 - box.minY * k + 8 })
  }

  // The route remounts per project (`remountDeps`), so `fit` and the wheel listener's `setView`
  // never belong to another project.
  useLayoutEffect(() => {
    if (!canvasViewport.get(project)) fit()
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- once per mount, which is per project
  }, [project])

  // The selected service stays in view beside the quick look.
  useLayoutEffect(() => {
    const el = viewport.current
    const node = graph.nodes.find((n) => n.id === selected)
    if (!el || !node || quickLookLeft(el) === undefined) return
    const margin = 24
    const right = el.clientWidth - covered(el) - margin
    const bottom = el.clientHeight - margin
    setView((v) => {
      const x = v.x + node.x * v.k
      const y = v.y + node.y * v.k
      const dx = Math.max(margin - x, Math.min(0, right - (x + node.w * v.k)))
      const dy = Math.max(margin - y, Math.min(0, bottom - (y + node.h * v.k)))
      return dx || dy ? { ...v, x: v.x + dx, y: v.y + dy } : v
    })
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- when another service is selected
  }, [selected])

  useEffect(() => {
    const el = viewport.current
    if (!el) return
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      const rect = el.getBoundingClientRect()
      const px = event.clientX - rect.left
      const py = event.clientY - rect.top
      setView((v) => {
        const k = Math.min(2.5, Math.max(0.25, v.k * Math.exp(-event.deltaY * 0.0015)))
        return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k }
      })
    }
    el.addEventListener("wheel", onWheel, { passive: false })
    return () => el.removeEventListener("wheel", onWheel)
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- the listener reads state through setView
  }, [])

  const zoom = (factor: number) => {
    const el = viewport.current
    if (!el) return
    const cx = el.clientWidth / 2
    const cy = el.clientHeight / 2
    setView((v) => {
      const k = Math.min(2.5, Math.max(0.25, v.k * factor))
      return { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k }
    })
  }

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("[data-node],button,a")) return
    drag.current = { x: event.clientX, y: event.clientY, vx: view.x, vy: view.y }
    event.currentTarget.setPointerCapture(event.pointerId)
    setDragging(true)
  }
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d) return
    setView((v) => ({ ...v, x: d.vx + event.clientX - d.x, y: d.vy + event.clientY - d.y }))
  }
  const onPointerUp = () => {
    drag.current = null
    setDragging(false)
  }

  const active = hoverNode ?? selected
  const related = (edge: LaidEdge) => !active || edge.from === active || edge.to === active
  const hovered = graph.edges.find((e) => e.id === hoverEdge)
  const control =
    "flex size-8 items-center justify-center text-muted-foreground outline-none hover:bg-muted hover:text-foreground"

  return (
    <div
      ref={viewport}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      className={cn(
        "absolute inset-0 touch-none overflow-hidden bg-dots select-none",
        dragging ? "cursor-grabbing" : "cursor-grab",
      )}
    >
      <div
        className="absolute top-0 left-0 origin-top-left translate-x-(--pan-x) translate-y-(--pan-y) scale-(--zoom)"
        style={{ "--pan-x": px(view.x), "--pan-y": px(view.y), "--zoom": view.k }}
      >
        {graph.frames.map((frame) => (
          <div
            key={frame.lane}
            className="absolute top-(--y) left-(--x) h-(--h) w-(--w) rounded-2xl border border-dashed border-foreground/12 bg-background/35"
            style={{
              "--x": px(frame.x),
              "--y": px(frame.y),
              "--w": px(frame.w),
              "--h": px(frame.h),
            }}
          >
            <div className="flex h-7.5 items-center gap-2 px-3 type-meta font-medium text-muted-foreground">
              {t(laneTitle[frame.lane])}
              <span className="flex items-center gap-1 tracking-normal normal-case">
                <StatusDot
                  health={frame.ready === frame.total ? "ok" : "fail"}
                  className="size-1.5"
                />
                {frame.ready}/{frame.total}
              </span>
            </div>
          </div>
        ))}

        <svg
          className="pointer-events-none absolute top-0 left-0 overflow-visible"
          width={graph.width}
          height={graph.height}
          aria-hidden="true"
        >
          <defs>
            <marker
              id="edge-end"
              viewBox="0 0 8 8"
              refX="7"
              refY="4"
              markerWidth="6"
              markerHeight="6"
              orient="auto"
            >
              <path d="M0,0.5 L7.5,4 L0,7.5 z" className="fill-foreground/45" />
            </marker>
          </defs>
          {graph.edges.map((edge) => (
            <g
              key={edge.id}
              className={cn("transition-opacity", related(edge) ? "opacity-100" : "opacity-20")}
            >
              <path
                d={edge.path}
                fill="none"
                className={cn(
                  edge.id === hoverEdge
                    ? "stroke-brand"
                    : edge.access === "blocked"
                      ? "stroke-fail"
                      : edge.access === "limited"
                        ? "stroke-warn"
                        : "stroke-foreground/45 dark:stroke-foreground/55",
                )}
                strokeWidth={edge.id === hoverEdge ? 2.25 : 1.5}
                strokeDasharray={
                  edge.access === "blocked" ? "1 4" : edge.inferred ? "5 5" : undefined
                }
                strokeLinecap={edge.access === "blocked" ? "round" : undefined}
                markerEnd="url(#edge-end)"
              />
              <path
                d={edge.path}
                fill="none"
                stroke="transparent"
                strokeWidth={14}
                className="pointer-events-auto cursor-help"
                onPointerEnter={() => setHoverEdge(edge.id)}
                onPointerLeave={() => setHoverEdge(undefined)}
              />
            </g>
          ))}
        </svg>

        {graph.nodes.map((node) => (
          <ServiceCard
            key={node.id}
            node={node}
            service={node.service}
            host={domainsOf(data, node.service)[0]?.host}
            selected={selected === node.id}
            faded={
              !!active &&
              active !== node.id &&
              !graph.edges.some(
                (e) =>
                  (e.from === active && e.to === node.id) ||
                  (e.to === active && e.from === node.id),
              )
            }
            onOpen={() => onOpen(node.id)}
            onHover={(on) => setHoverNode(on ? node.id : undefined)}
            word={word}
          />
        ))}

        {hovered ? (
          <div
            className="pointer-events-none absolute top-(--y) left-(--x) z-10 -translate-x-1/2 -translate-y-tooltip rounded-lg border border-border-popover bg-popover px-2 py-1 type-micro font-medium whitespace-nowrap text-popover-foreground shadow-xs"
            style={{ "--x": px(hovered.mid.x), "--y": px(hovered.mid.y) }}
          >
            <span className="mr-1.5 opacity-60">
              {hovered.inferred ? t("edge.inferred") : t("edge.declared")}
            </span>
            {edgeText(hovered, t)}
            {hovered.access && hovered.access !== "allowed" ? (
              <span className="mt-0.5 block opacity-80">
                {hovered.access === "blocked" ? t("reach.edgeBlocked") : t("reach.edgeLimited")}
              </span>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="absolute top-3 left-3 z-10 flex flex-col overflow-hidden rounded-lg border bg-background shadow-xs">
        <button
          type="button"
          className={control}
          onClick={() => zoom(1.2)}
          aria-label={t("canvas.zoomIn")}
          title={t("canvas.zoomIn")}
        >
          <PlusIcon className="size-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          className={cn(control, "border-t")}
          onClick={() => zoom(1 / 1.2)}
          aria-label={t("canvas.zoomOut")}
          title={t("canvas.zoomOut")}
        >
          <MinusIcon className="size-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          className={cn(control, "border-t")}
          onClick={fit}
          aria-label={t("canvas.fit")}
          title={t("canvas.fit")}
        >
          <CornersOutIcon className="size-4" aria-hidden="true" />
        </button>
      </div>
      <div className="absolute bottom-3 left-3 z-10 flex items-center gap-3 rounded-lg border bg-background px-3 py-1.5 type-micro text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <svg width="20" height="6" aria-hidden="true">
            <line x1="0" x2="20" y1="3" y2="3" className="stroke-foreground/60" strokeWidth="1.5" />
          </svg>
          {t("edge.declared")}
        </span>
        <span className="flex items-center gap-1.5">
          <svg width="20" height="6" aria-hidden="true">
            <line
              x1="0"
              x2="20"
              y1="3"
              y2="3"
              className="stroke-foreground/60"
              strokeWidth="1.5"
              strokeDasharray="4 3"
            />
          </svg>
          {t("edge.inferred")}
        </span>
        {graph.edges.some((e) => e.access === "blocked") ? (
          <span className="flex items-center gap-1.5">
            <svg width="20" height="6" aria-hidden="true">
              <line
                x1="1"
                x2="19"
                y1="3"
                y2="3"
                className="stroke-fail"
                strokeWidth="1.5"
                strokeDasharray="1 4"
                strokeLinecap="round"
              />
            </svg>
            {t("reach.blocked")}
          </span>
        ) : null}
        <Hint label={t("canvas.legend")}>{t("canvas.legendHint")}</Hint>
        <span className="hidden tabular-nums sm:inline">{Math.round(view.k * 100)}%</span>
      </div>
    </div>
  )
}

function ServiceCard({
  node,
  service,
  selected,
  faded,
  onOpen,
  onHover,
  word,
  host,
}: {
  node: LaidNode
  service: Service
  host?: string
  selected: boolean
  faded: boolean
  onOpen: () => void
  onHover: (on: boolean) => void
  word: ReturnType<typeof useStatusWord>
}) {
  const t = useT()
  const status = statusOf(service)
  const volumes = volumesOf(service)
  const first = service.diagnoses?.[0]
  return (
    <div
      data-node
      className={cn(
        "absolute top-(--y) left-(--x) w-(--w) transition-opacity",
        faded && "opacity-55",
      )}
      style={{ "--x": px(node.x), "--y": px(node.y), "--w": px(node.w) }}
      onPointerEnter={() => onHover(true)}
      onPointerLeave={() => onHover(false)}
    >
      <button
        type="button"
        onClick={onOpen}
        aria-pressed={selected}
        className={cn(
          "relative z-1 flex h-28 w-full flex-col gap-2 rounded-lg border bg-card p-3 text-left shadow-xs outline-none transition-colors hover:border-foreground/25",
          selected && "border-brand ring-3 ring-brand/25",
          status.health === "fail" && !selected && "border-fail/40",
        )}
      >
        <span className="flex items-center gap-2.5">
          <ServiceIcon service={service} />
          <span className="flex min-w-0 flex-1 flex-col leading-tight">
            <span className="truncate text-sm leading-tight font-semibold" translate="no">
              {service.displayName ?? service.name}
            </span>
            {host || service.image ? (
              <span
                className="truncate font-mono type-micro leading-tight text-muted-foreground"
                translate="no"
              >
                {host ?? imageTag(service.image)}
              </span>
            ) : null}
          </span>
          {service.provenance.tool ? (
            <GitBranchIcon
              className="size-3.5 text-muted-foreground"
              aria-label={service.provenance.tool}
            />
          ) : null}
        </span>
        {first ? (
          <span className="flex min-w-0">
            <Badge variant="fail">{t(`origin.${originOf(first).origin}`)}</Badge>
          </span>
        ) : null}
        <span className="mt-auto flex items-center gap-2 text-xs">
          <StatusDot health={status.health} />
          <span className={cn("truncate font-medium", status.health === "fail" && "text-fail")}>
            {word(status)}
          </span>
          {status.desired !== undefined ? (
            <span className="ml-auto text-muted-foreground tabular-nums">
              {status.ready}/{status.desired}
            </span>
          ) : null}
        </span>
      </button>
      {volumes.map((m) => (
        <div
          key={claimOf(m)}
          className="relative mx-2 -mt-2 flex h-9.5 items-end gap-2 rounded-b-xl border border-t-0 bg-muted/70 px-3 pb-2 type-micro leading-normal text-muted-foreground"
        >
          <HardDriveIcon className="size-3.5" aria-hidden="true" />
          <span className="truncate font-mono" translate="no">
            {claimOf(m)}
          </span>
          <span className="ml-auto font-medium text-foreground/70 tabular-nums">{m.size}</span>
        </div>
      ))}
    </div>
  )
}
