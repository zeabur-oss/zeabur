import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useVirtualizer } from "@tanstack/react-virtual"
import { getRouteApi, Link } from "@tanstack/react-router"
import { lazy, Suspense, useRef, useState, type ReactNode } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import PlusIcon from "~icons/ph/plus"
import PushPinIcon from "~icons/ph/push-pin"
import PushPinFillIcon from "~icons/ph/push-pin-fill"

import type { Schemas } from "@/api/client"
import { objectsQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { UpdatedAt } from "@/components/freshness"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { permission, Refused, RefusedSome } from "@/components/refused"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { parseObjectKey } from "@/lib/objects"
import { pinnedKinds, togglePinnedKind } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { ProjectHeader } from "@/routes/project/header"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { PageColumn } from "@zeabur/design-system/components/page-column"

const ObjectSheet = lazy(() =>
  import("@/components/object-sheet").then((m) => ({ default: m.ObjectSheet })),
)

const route = getRouteApi("/projects/$project/objects")

type Kind = Schemas["ObjectKind"]

type Row =
  | {
      type: "kind"
      kind: Kind
      key: string
      count?: number
      loading: boolean
      refused: boolean
    }
  | { type: "object"; kind: Kind; object: Schemas["ObjectSummary"] }
  | { type: "refused"; kind: Kind; refused: Schemas["UnreadableView"] }

const HEAD = 40
const ITEM = 34
/** Estimates: a refusal row grows when its details open, and the list measures it. */
const sizes: Record<Row["type"], number> = { kind: HEAD, object: ITEM, refused: 46 }

/** As `unreadable` names a kind. */
const resourceKey = (kind: Pick<Kind, "group" | "resource">) =>
  kind.group ? `${kind.resource}.${kind.group}` : kind.resource

/** As the API names a kind. */
const kindKey = (kind: Pick<Kind, "group" | "resource">) =>
  `${kind.group || "core"}/${kind.resource}`

export function ObjectsPage() {
  const t = useT()
  const { project } = route.useParams()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const client = useQueryClient()
  const pins = pinnedKinds.useValue()
  const [query, setQuery] = useState(search.q ?? "")

  const base = useQuery(objectsQuery(project))
  const curated = new Set(base.data?.items.filter((k) => k.curated).map(kindKey))
  const served = new Set(base.data?.items.map(kindKey))
  const pinned = pins.filter((k) => served.has(k) && !curated.has(k))
  const pinnedRead = useQuery({
    ...objectsQuery(project, { kinds: pinned }),
    enabled: base.isSuccess && pinned.length > 0,
  })
  const scanned = useQuery({ ...objectsQuery(project, { all: true }), enabled: base.isSuccess })

  const set = (patch: { q?: string; object?: string; generated?: boolean; empty?: boolean }) =>
    void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: !patch.object })
  const address = search.object ? parseObjectKey(search.object) : undefined

  const reads = [pinnedRead, scanned]
  const kinds = new Map<string, Kind>()
  const unreadable = new Map<string, Schemas["UnreadableView"]>()
  for (const answer of [base, ...reads]) {
    for (const kind of answer.data?.items ?? [])
      if (kind.listed || !kinds.has(kindKey(kind))) kinds.set(kindKey(kind), kind)
    for (const refused of answer.data?.unreadable ?? []) unreadable.set(refused.resource, refused)
  }
  const loading = new Set(
    [
      ...(pinnedRead.isFetching ? pinned : []),
      ...(scanned.isFetching ? [...kinds.keys()] : []),
    ].filter((k) => !kinds.get(k)?.listed),
  )
  const updated = base.dataUpdatedAt ? new Date(base.dataUpdatedAt).toISOString() : undefined

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ProjectHeader
        entry="objects"
        actions={
          <Link
            to="/projects/$project/create"
            params={{ project }}
            className={buttonVariants({ size: "sm" })}
          >
            <PlusIcon aria-hidden="true" data-icon="inline-start" />
            {t("create.button")}
          </Link>
        }
      />
      <PageColumn gap="sm" className="mb-8 min-h-0 flex-1">
        {base.isPending ? (
          <LoadingRows label={t("common.loading")} rows={8} />
        ) : base.isError ? (
          <Problem error={base.error} />
        ) : (
          <ObjectList
            kinds={[...kinds.values()]}
            unreadable={[...unreadable.values()]}
            loading={loading}
            pins={pins}
            scanning={scanned.isFetching}
            updated={updated}
            refreshing={base.isFetching}
            query={query}
            onQuery={(value) => {
              setQuery(value)
              set({ q: value || undefined })
            }}
            generated={search.generated ?? false}
            onGenerated={(value) => set({ generated: value || undefined })}
            empty={search.empty ?? false}
            onEmpty={(value) => set({ empty: value || undefined })}
            onRefresh={() =>
              void client.invalidateQueries({ queryKey: ["project", project, "objects"] })
            }
            onOpen={(key) => set({ object: key })}
          />
        )}
      </PageColumn>
      {address ? (
        <Suspense fallback={null}>
          <ObjectSheet
            project={project}
            address={address}
            onClose={() =>
              void navigate({ search: (previous) => ({ ...previous, object: undefined }) })
            }
          />
        </Suspense>
      ) : null}
    </div>
  )
}

function ObjectList({
  kinds,
  unreadable,
  loading,
  pins,
  scanning,
  updated,
  refreshing,
  query,
  onQuery,
  generated,
  onGenerated,
  empty,
  onEmpty,
  onRefresh,
  onOpen,
}: {
  kinds: Kind[]
  unreadable: Schemas["UnreadableView"][]
  loading: Set<string>
  pins: string[]
  scanning: boolean
  updated?: string
  refreshing: boolean
  query: string
  onQuery: (value: string) => void
  generated: boolean
  onGenerated: (value: boolean) => void
  empty: boolean
  onEmpty: (value: boolean) => void
  onRefresh: () => void
  onOpen: (key: string) => void
}) {
  const t = useT()
  const scroller = useRef<HTMLDivElement>(null)
  const needle = query.trim().toLowerCase()
  const folded = kinds.filter((k) => k.generated)
  const emptyKinds = kinds.filter((k) => k.listed && k.items.length === 0)
  const refusedBy = new Map(unreadable.map((u) => [u.resource, u]))
  const isPinned = (k: Kind) => pins.includes(kindKey(k))
  const order = (k: Kind) => (isPinned(k) ? 0 : k.listed || refusedBy.has(resourceKey(k)) ? 1 : 2)
  const shown = kinds
    .filter((k) => isPinned(k) || generated || !k.generated)
    .filter((k) => isPinned(k) || empty || !(k.listed && k.items.length === 0))
    .toSorted((a, b) => order(a) - order(b) || a.kind.localeCompare(b.kind))
  const rows: Row[] = []
  for (const kind of shown) {
    const key = kindKey(kind)
    const kindMatches =
      !needle ||
      `${kind.kind} ${kindName(t, kind.kind)} ${kind.group}`.toLowerCase().includes(needle)
    const matching = kind.items.filter((o) => kindMatches || o.name.toLowerCase().includes(needle))
    if (!kindMatches && matching.length === 0) continue
    const refused = refusedBy.get(resourceKey(kind))
    rows.push({
      type: "kind",
      kind,
      key,
      count: kind.listed ? matching.length : undefined,
      loading: loading.has(key),
      refused: refused !== undefined,
    })
    if (refused) rows.push({ type: "refused", kind, refused })
    for (const object of matching) rows.push({ type: "object", kind, object })
  }
  const placed = new Set(rows.flatMap((r) => (r.type === "refused" ? [r.refused.resource] : [])))
  const total = kinds.reduce((n, k) => n + k.items.length, 0)

  // oxlint-disable-next-line react/incompatible-library -- the virtualizer returns new functions each render
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (index) => sizes[rows[index]?.type ?? "object"],
    overscan: 12,
  })

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring sm:w-64">
          <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            aria-label={t("objects.filter")}
            placeholder={t("objects.filter")}
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </label>
        <Toggle pressed={generated} onChange={onGenerated} hidden={folded.length === 0}>
          {t("objects.generated")}
          <span className="text-muted-foreground tabular-nums">{folded.length}</span>
        </Toggle>
        <Toggle pressed={empty} onChange={onEmpty} hidden={emptyKinds.length === 0}>
          {t("objects.empty")}
          <span className="text-muted-foreground tabular-nums">{emptyKinds.length}</span>
        </Toggle>
        {scanning ? (
          <span className="type-meta text-muted-foreground">{t("objects.scanning")}</span>
        ) : null}
        <span className="ml-auto flex items-center gap-2 type-meta text-muted-foreground tabular-nums">
          {t("objects.count", total)}
          {updated ? (
            <span className="ml-1">
              <UpdatedAt at={Date.parse(updated)} />
            </span>
          ) : null}
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onRefresh}
            disabled={refreshing}
            aria-busy={refreshing}
            aria-label={t("objects.refresh")}
            title={t("objects.refresh")}
          >
            <ArrowClockwiseIcon
              aria-hidden="true"
              className={cn(refreshing && "motion-safe:animate-spin motion-reduce:animate-pulse")}
            />
          </Button>
        </span>
      </div>
      <RefusedSome unreadable={unreadable.filter((u) => !placed.has(u.resource))} />
      {rows.length === 0 ? (
        <Empty size="sm">
          <EmptyHeader>
            <EmptyMedia>
              <Illustration kind={needle ? "no-results" : "empty"} seed="objects" />
            </EmptyMedia>
            <EmptyDescription>
              {needle ? t("objects.noMatch", query.trim()) : t("objects.none")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div
          ref={scroller}
          className="min-h-96 flex-1 overflow-y-auto rounded-xl border border-v2-border bg-card"
          role="list"
          aria-label={t("project.entry.objects")}
        >
          <div
            className="relative w-full h-(--total)"
            style={{ "--total": `${virtualizer.getTotalSize()}px` }}
          >
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index]
              if (!row) return null
              return (
                <div
                  key={item.key}
                  ref={virtualizer.measureElement}
                  data-index={item.index}
                  role="listitem"
                  className="absolute inset-x-0 top-0 translate-y-(--y)"
                  style={{ "--y": `${item.start}px` }}
                >
                  {row.type === "kind" ? (
                    <KindRow row={row} pinned={pins.includes(row.key)} />
                  ) : row.type === "refused" ? (
                    <div className="border-b px-4 py-2">
                      <Refused
                        refusal="unreadable"
                        what={t("refused.listThese")}
                        missing={[permission("list", row.kind.resource, row.kind.group)]}
                        line
                      >
                        <p className="font-mono text-xs text-muted-foreground">
                          {row.refused.kubernetes.message}
                        </p>
                      </Refused>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() =>
                        onOpen(row.object.id.replace(/^projects\/[^/]+\/objects\//, ""))
                      }
                      className="flex h-8.5 w-full items-center gap-2 px-4 text-left text-sm outline-none hover:bg-hover"
                    >
                      <CaretRightIcon
                        className="size-3 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                      />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs" translate="no">
                        {row.object.name}
                      </span>
                      {row.object.ownerKind ? (
                        <span className="hidden type-meta text-muted-foreground sm:inline">
                          {t("objects.owner", kindName(t, row.object.ownerKind))}
                        </span>
                      ) : null}
                      <Ago iso={row.object.created} className="type-meta text-muted-foreground" />
                    </button>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      )}
    </>
  )
}

function Toggle({
  pressed,
  onChange,
  hidden,
  children,
}: {
  pressed: boolean
  onChange: (value: boolean) => void
  hidden: boolean
  children: ReactNode
}) {
  if (hidden) return null
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={() => onChange(!pressed)}
      className={cn(
        "inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs outline-none",
        pressed ? "bg-muted font-medium" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  )
}

function KindRow({ row, pinned }: { row: Extract<Row, { type: "kind" }>; pinned: boolean }) {
  const t = useT()
  const { kind } = row
  const plain = kindName(t, kind.kind)
  const denied = kind.access === "denied"
  const label = (
    <>
      {kind.listed ? (
        <CaretDownIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />
      ) : (
        <CaretRightIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />
      )}
      <span className="text-sm font-medium" translate={plain === kind.kind ? "no" : undefined}>
        {plain}
      </span>
      {plain === kind.kind ? null : (
        <span className="truncate type-meta text-muted-foreground" translate="no">
          {kind.kind}
        </span>
      )}
    </>
  )
  return (
    <div className="flex h-10 items-center gap-2 border-b bg-muted/30 px-4">
      <span
        className="flex min-w-0 flex-1 items-center gap-2"
        title={kind.group ? `${kind.group}/${kind.version}` : kind.version}
      >
        {label}
      </span>
      <span className="type-meta text-muted-foreground tabular-nums">
        {row.refused
          ? null
          : denied
            ? t("objects.denied")
            : row.loading
              ? t("common.loading")
              : row.count !== undefined
                ? row.count
                : null}
      </span>
      <button
        type="button"
        aria-pressed={pinned}
        aria-label={pinned ? t("objects.unpin", plain) : t("objects.pin", plain)}
        title={pinned ? t("objects.unpin", plain) : t("objects.pin", plain)}
        onClick={() => togglePinnedKind(row.key)}
        className={cn(
          "-mr-1 rounded p-1 outline-none hover:bg-muted",
          pinned ? "text-foreground" : "text-muted-foreground/60",
        )}
      >
        {pinned ? (
          <PushPinFillIcon className="size-3.5" aria-hidden="true" />
        ) : (
          <PushPinIcon className="size-3.5" aria-hidden="true" />
        )}
      </button>
    </div>
  )
}
