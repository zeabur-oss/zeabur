import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query"
import { lazy, Suspense, useDeferredValue, useRef, useState, type ReactNode } from "react"
import ArrowUpIcon from "~icons/ph/arrow-up"
import CaretRightIcon from "~icons/ph/caret-right"
import ClockCountdownIcon from "~icons/ph/clock-countdown"
import CubeIcon from "~icons/ph/cube"
import DotsThreeIcon from "~icons/ph/dots-three"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import EyeIcon from "~icons/ph/eye"
import FileIcon from "~icons/ph/file"
import FolderIcon from "~icons/ph/folder"
import FolderPlusIcon from "~icons/ph/folder-plus"
import GearIcon from "~icons/ph/gear"
import HardDrivesIcon from "~icons/ph/hard-drives"
import LinkSimpleIcon from "~icons/ph/link-simple"
import LockSimpleIcon from "~icons/ph/lock-simple"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import TrashIcon from "~icons/ph/trash"
import UploadSimpleIcon from "~icons/ph/upload-simple"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import {
  api,
  ApiError,
  localProblem,
  toProblem,
  unwrap,
  type Problem as ProblemBody,
  type Schemas,
} from "@/api/client"
import { debugQuery, whoamiQuery } from "@/api/queries"
import { REQUEST_HEADERS } from "@/auth/session"
import { Ago, Hint } from "@/components/bits"
import { FilterField } from "@/components/filter-field"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { isRefusal, permission, Refused } from "@/components/refused"
import { YamlBlock } from "@/components/yaml"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Card } from "@zeabur/design-system/components/card"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Input } from "@zeabur/design-system/components/input"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"
import { Select } from "@zeabur/design-system/components/select"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT, type Translate } from "@/i18n"
import { cn } from "@/lib/utils"
import type { Service } from "@/model/service"
import { execRole } from "@/model/terminal"

const DataEditor = lazy(() =>
  import("@/components/data-editor").then((m) => ({ default: m.DataEditor })),
)

type Files = Schemas["Files"]
type Mount = Schemas["FileMount"]
type Entry = Schemas["FileEntry"]
type Kind = Schemas["FileMountKind"]

/** Where the calls go: the pod, its container, and the debug container that reads for it. */
interface Place {
  project: string
  pod: string
  container: string
  via?: string
}

const base = (p: Place) =>
  `/api/v1/projects/${encodeURIComponent(p.project)}/pods/${encodeURIComponent(p.pod)}/files`

const query = (p: Place, extra: Record<string, string | undefined>) => {
  const params = new URLSearchParams()
  for (const [name, value] of Object.entries({ container: p.container, via: p.via, ...extra }))
    if (value) params.set(name, value)
  return params.toString()
}

const contentUrl = (p: Place, path: string) => `${base(p)}/content?${query(p, { path })}`

const filesKey = (project: string, pod: string) => ["project", project, "files", pod]

const filesQuery = (project: string, pod: string, container: string) =>
  queryOptions({
    queryKey: [...filesKey(project, pod), container],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/pods/{pod}/files", {
          params: { path: { project, pod }, query: { container } },
          signal,
        }),
      ),
    staleTime: 0,
  })

const listQuery = (p: Place, path: string, filter: string) =>
  queryOptions({
    queryKey: [...filesKey(p.project, p.pod), p.container, p.via, "list", path, filter],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/pods/{pod}/files/entries", {
          params: {
            path: { project: p.project, pod: p.pod },
            query: { container: p.container, via: p.via, path, filter: filter || undefined },
          },
          signal,
        }),
      ),
    staleTime: 5_000,
  })

const previewQuery = (p: Place, path: string) =>
  queryOptions({
    queryKey: [...filesKey(p.project, p.pod), p.container, p.via, "preview", path],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/pods/{pod}/files/preview", {
          params: {
            path: { project: p.project, pod: p.pod },
            query: { container: p.container, via: p.via, path },
          },
          signal,
        }),
      ),
    staleTime: 0,
  })

const join = (folder: string, name: string) => (folder === "/" ? `/${name}` : `${folder}/${name}`)
const parentOf = (path: string) => path.slice(0, path.lastIndexOf("/")) || "/"
const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1)
const within = (path: string, folder: string) =>
  folder === "/" || path === folder || path.startsWith(`${folder}/`)

/** The mount a path is in: the deepest that holds it. */
function mountOf(mounts: Mount[], path: string): Mount | undefined {
  return mounts
    .filter((m) => within(path, m.path))
    .reduce<Mount | undefined>((a, m) => (!a || m.path.length > a.path.length ? m : a), undefined)
}

/** A mount point, or a folder that holds one, stays where the pod mounts it. */
const holdsMount = (mounts: Mount[], path: string) =>
  path === "/" || mounts.some((m) => m.path !== "/" && within(m.path, path))

const kernel = ["/proc", "/sys", "/dev"]

function useSize() {
  const { locale } = useI18n()
  const units = ["B", "kB", "MB", "GB", "TB"]
  return (bytes: number) => {
    let value = bytes
    let unit = 0
    while (value >= 1000 && unit < units.length - 1) {
      value /= 1000
      unit += 1
    }
    const number = new Intl.NumberFormat(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 })
    return `${number.format(value)} ${units[unit]}`
  }
}

const isoOf = (seconds: number) => new Date(seconds * 1000).toISOString()

export function FilesTab({
  project,
  service,
  pod: asked,
  onPodChange,
}: {
  project: string
  service: Service
  pod?: string
  onPodChange: (pod?: string) => void
}) {
  const t = useT()
  const running = service.pods.filter((p) => p.phase === "Running")
  const pod = running.find((p) => p.name === asked) ?? running[0]
  const main = new Set(service.containers.filter((c) => !c.init).map((c) => c.name))
  const containers = (pod?.containers ?? []).filter((c) => main.size === 0 || main.has(c))
  const [chosen, setChosen] = useState<string>()
  const container = containers.find((c) => c === chosen) ?? containers[0]
  if (!pod || !container) return <NotRunning />
  return (
    <div className="flex flex-col gap-4">
      {running.length > 1 || containers.length > 1 ? (
        <div className="flex flex-wrap items-center gap-2">
          {running.length > 1 ? (
            <Select
              size="sm"
              value={pod.name}
              items={running.map((p) => ({ value: p.name, label: p.name }))}
              onValueChange={(value) => onPodChange(value)}
              aria-label={t("files.replica")}
              translate="no"
            />
          ) : null}
          {containers.length > 1 ? (
            <Select
              size="sm"
              value={container}
              items={containers.map((c) => ({ value: c, label: c }))}
              onValueChange={setChosen}
              aria-label={t("files.container")}
              translate="no"
            />
          ) : null}
        </div>
      ) : null}
      <Browser
        key={`${pod.name}/${container}`}
        project={project}
        pod={pod.name}
        container={container}
      />
    </div>
  )
}

function NotRunning() {
  const t = useT()
  return (
    <div className="flex flex-col gap-1.5 rounded-xl border border-dashed px-4 py-3 text-sm">
      <p className="font-medium">{t("files.notRunningTitle")}</p>
      <p className="text-muted-foreground">{t("files.notRunningBody")}</p>
    </div>
  )
}

function Browser({ project, pod, container }: { project: string; pod: string; container: string }) {
  const t = useT()
  const [waiting, setWaiting] = useState(false)
  const files = useQuery({
    ...filesQuery(project, pod, container),
    refetchInterval: (q) => (waiting && q.state.data?.reader.kind === "none" ? 1500 : false),
  })
  if (files.isPending) return <LoadingRows label={t("files.loading")} />
  if (files.isError) {
    const problem = toProblem(files.error)
    if (isRefusal(problem))
      return <RefusedFiles project={project} problem={problem} retry={() => void files.refetch()} />
    return <Problem problem={problem} retry={() => void files.refetch()} />
  }
  const data = files.data
  const place: Place = { project, pod, container, via: data.reader.via ?? undefined }
  if (data.reader.kind === "none")
    return (
      <div className="flex flex-col gap-4">
        <NoTools place={place} files={data} waiting={waiting} onWaiting={() => setWaiting(true)} />
        <Card variant="list">
          <FolderHeader mounts={data.mounts} onOpen={() => undefined} />
          <MountTable files={data} onOpen={undefined} />
        </Card>
      </div>
    )
  return <Folders place={place} files={data} />
}

function RefusedFiles({
  project,
  problem,
  retry,
}: {
  project: string
  problem: ProblemBody
  retry: () => void
}) {
  const t = useT()
  const exec = problem.denied?.resource === "pods/exec"
  const whoami = useQuery({ ...whoamiQuery, enabled: exec })
  return (
    <Refused refusal={problem} what={t("files.refusedWhat")} retry={retry}>
      {exec ? (
        <>
          <p className="text-sm text-muted-foreground">{t("files.refusedRbac")}</p>
          <YamlBlock
            yaml={execRole(project, whoami.data?.username || t("access.you"))}
            label={t("files.roleLabel")}
          />
        </>
      ) : null}
    </Refused>
  )
}

/** The image has no tar: its volumes can be read through a debug container that mounts them. */
function NoTools({
  place,
  files,
  waiting,
  onWaiting,
}: {
  place: Place
  files: Files
  waiting: boolean
  onWaiting: () => void
}) {
  const t = useT()
  const debug = useQuery({
    ...debugQuery(place.project, place.pod, place.container),
    enabled: waiting,
    refetchInterval: (q) => (waiting && !q.state.data?.problem ? 1500 : false),
  })
  const { plan, dialog } = usePlanner(place.project, onWaiting, true)
  const problem = waiting ? (debug.data?.problem ?? undefined) : undefined
  const may = files.reader.canAddDebugContainer !== false
  const start = () =>
    plan({
      title: t("files.addDebug"),
      changes: [{ op: "debugContainer", pod: place.pod, container: place.container }],
      quiet: true,
    })
  return (
    <div className="flex flex-col items-start gap-3 rounded-xl border border-dashed px-4 py-3 text-sm">
      <p className="font-medium">{t("files.noToolsTitle")}</p>
      <p className="max-w-prose text-muted-foreground">{t("files.noToolsBody")}</p>
      {problem ? <Problem problem={problem} retry={start} /> : null}
      {!may ? (
        <Refused
          refusal="hint"
          what={t("files.addDebugWhat")}
          missing={[permission("patch", "pods/ephemeralcontainers")]}
          line
        />
      ) : waiting && !problem ? (
        <output className="text-muted-foreground">{t("files.debugStarting")}</output>
      ) : (
        <>
          <p className="max-w-prose text-muted-foreground">{t("files.addDebugBody")}</p>
          <Button variant="outline" size="sm" onClick={start}>
            <CubeIcon aria-hidden="true" data-icon="inline-start" />
            {t("files.addDebug")}
          </Button>
        </>
      )}
      {dialog}
    </div>
  )
}

const kindIcon: Record<Kind, typeof HardDrivesIcon> = {
  claim: HardDrivesIcon,
  configMap: GearIcon,
  secret: LockSimpleIcon,
  emptyDir: ClockCountdownIcon,
  image: CubeIcon,
  other: FolderIcon,
}

/** What a mount keeps: read-only, lost when replaced or on restart, or nothing to say. */
function keeps(t: Translate, mount: Mount): string | undefined {
  if (mount.readOnly) return t("files.readOnly")
  if (mount.lost === "replaced") return t("files.lostOnReplace")
  if (mount.lost === "restart") return t("files.lostOnRestart")
  return undefined
}

const head = "px-3 py-2 text-left type-meta font-medium text-muted-foreground whitespace-nowrap"
const cell = "px-3 py-2 align-middle max-sm:p-0"
const thead = "border-b bg-muted/30 max-sm:hidden"
const row =
  "max-sm:flex max-sm:flex-wrap max-sm:items-center max-sm:gap-x-3 max-sm:gap-y-1 max-sm:px-4 max-sm:py-3"
const opener =
  "flex max-w-full min-w-0 items-center gap-2 rounded-sm text-left hover:underline disabled:no-underline"

/** The first level: the volumes, then the image's own files. */
function MountTable({ files, onOpen }: { files: Files; onOpen?: (path: string) => void }) {
  const t = useT()
  const viaDebug = files.reader.kind === "debugContainer"
  return (
    <table className="w-full text-sm max-sm:block" aria-label={t("files.volumes")}>
      <thead className={thead}>
        <tr>
          <th className={`${head} sm:w-1/2 sm:pl-4`}>{t("files.path")}</th>
          <th className={head}>{t("files.what")}</th>
          <th className={`${head} sm:pr-4 text-right`}>
            <span className="sr-only">{t("files.keeps")}</span>
          </th>
        </tr>
      </thead>
      <tbody className="divide-y max-sm:block">
        {files.mounts.map((mount) => {
          const Icon = kindIcon[mount.kind]
          const unreachable = viaDebug && mount.kind === "image"
          const open = onOpen && !unreachable ? () => onOpen(mount.path) : undefined
          const label = (
            <>
              <Icon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
              <span className="truncate font-mono" translate="no">
                {mount.path}
              </span>
            </>
          )
          return (
            <tr key={mount.path} className={row}>
              <td className={`${cell} max-w-0 sm:pl-4 max-sm:w-full max-sm:max-w-none`}>
                {open ? (
                  <button type="button" onClick={open} className={opener}>
                    {label}
                  </button>
                ) : (
                  <span className="flex min-w-0 items-center gap-2 text-muted-foreground">
                    {label}
                  </span>
                )}
              </td>
              <td className={`${cell} text-muted-foreground sm:whitespace-nowrap`}>
                <Tooltip content={t(`files.kindHint.${mount.kind}`)}>
                  <span className="cursor-help">{t(`files.kind.${mount.kind}`)}</span>
                </Tooltip>
                {mount.object ? (
                  <span className="font-mono" translate="no">
                    {" "}
                    {mount.object}
                  </span>
                ) : null}
                {mount.part ? <span> {t("files.part")}</span> : null}
              </td>
              <td
                className={`${cell} sm:pr-4 text-right text-muted-foreground sm:whitespace-nowrap max-sm:text-left`}
              >
                {unreachable
                  ? t("files.imageUnreachable")
                  : files.reader.kind === "none"
                    ? t("files.notListed")
                    : keeps(t, mount)}
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

/** The path as a breadcrumb, starting at the volumes; above a mount is not its business. */
function FolderHeader({
  mounts,
  path,
  onOpen,
  actions,
}: {
  mounts: Mount[]
  path?: string
  onOpen: (path: string | undefined) => void
  actions?: ReactNode
}) {
  const t = useT()
  const mount = path === undefined ? undefined : mountOf(mounts, path)
  const crumbs: string[] = []
  if (path !== undefined && mount) {
    crumbs.push(mount.path)
    let at = mount.path
    for (const part of path.slice(mount.path.length).split("/").filter(Boolean)) {
      at = join(at, part)
      crumbs.push(at)
    }
  }
  return (
    <div className="flex min-h-11 flex-wrap items-center gap-x-2 gap-y-1.5 px-3 py-1.5">
      {path !== undefined ? (
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label={t("files.up")}
          onClick={() => onOpen(path === mount?.path ? undefined : parentOf(path))}
        >
          <ArrowUpIcon aria-hidden="true" />
        </Button>
      ) : null}
      <nav aria-label={t("files.location")} className="min-w-0 flex-1">
        <ol className="flex min-w-0 flex-wrap items-center gap-0.5 text-sm">
          <li className="flex items-center gap-0.5">
            {path === undefined ? (
              <span aria-current="location" className="px-1 font-medium">
                {t("files.volumes")}
              </span>
            ) : (
              <>
                <Button variant="ghost" size="xs" onClick={() => onOpen(undefined)}>
                  {t("files.volumes")}
                </Button>
                <CaretRightIcon aria-hidden="true" className="size-3 text-muted-foreground" />
              </>
            )}
          </li>
          {crumbs.map((crumb, index) => {
            const last = index === crumbs.length - 1
            const label = index === 0 ? crumb : nameOf(crumb)
            return (
              <li
                key={crumb}
                className="flex min-w-0 items-center gap-0.5 font-mono"
                translate="no"
              >
                {last ? (
                  <span aria-current="location" className="truncate px-2 font-medium">
                    {label}
                  </span>
                ) : (
                  <>
                    <Button variant="ghost" size="xs" onClick={() => onOpen(crumb)}>
                      {label}
                    </Button>
                    <CaretRightIcon aria-hidden="true" className="size-3 text-muted-foreground" />
                  </>
                )}
              </li>
            )
          })}
        </ol>
      </nav>
      {actions ? <div className="flex items-center gap-1.5 max-sm:w-full">{actions}</div> : null}
    </div>
  )
}

type Dialogs =
  | { kind: "preview"; path: string }
  | { kind: "rename"; entry: Entry; path: string }
  | { kind: "delete"; entry: Entry; path: string }
  | { kind: "folder" }
  | { kind: "upload"; files: File[] }
  | { kind: "editor"; editor: "ConfigMap" | "Secret"; name: string }

function Folders({ place, files }: { place: Place; files: Files }) {
  const t = useT()
  const [path, setPath] = useState<string>()
  const [filter, setFilter] = useState("")
  const [dialog, setDialog] = useState<Dialogs>()
  const [uploads, setUploads] = useState<Upload[]>([])
  const open = (next: string | undefined) => {
    setFilter("")
    setPath(next)
  }
  const close = () => setDialog(undefined)
  return (
    <div className="flex min-w-0 flex-col gap-3">
      {place.via ? (
        <p className="text-sm text-muted-foreground">
          {t("files.viaDebug")}{" "}
          <span className="font-mono" translate="no">
            {place.via}
          </span>
        </p>
      ) : null}
      {uploads.length > 0 ? (
        <UploadProgress uploads={uploads} onDismiss={() => setUploads([])} />
      ) : null}
      {path === undefined ? (
        <Card variant="list">
          <FolderHeader mounts={files.mounts} onOpen={open} />
          <MountTable files={files} onOpen={open} />
        </Card>
      ) : (
        <Folder
          place={place}
          files={files}
          path={path}
          filter={filter}
          onFilter={setFilter}
          onOpen={open}
          onDialog={setDialog}
        />
      )}
      {dialog?.kind === "preview" ? (
        <Preview place={place} path={dialog.path} onClose={close} />
      ) : dialog?.kind === "rename" ? (
        <RenameDialog place={place} path={dialog.path} onClose={close} />
      ) : dialog?.kind === "delete" ? (
        <DeleteDialog
          place={place}
          files={files}
          entry={dialog.entry}
          path={dialog.path}
          onClose={close}
        />
      ) : dialog?.kind === "folder" && path !== undefined ? (
        <FolderDialog place={place} files={files} folder={path} onClose={close} />
      ) : dialog?.kind === "upload" && path !== undefined ? (
        <UploadDialog
          place={place}
          files={files}
          folder={path}
          chosen={dialog.files}
          onClose={close}
          onProgress={setUploads}
        />
      ) : dialog?.kind === "editor" ? (
        <Suspense fallback={null}>
          <DataEditor
            project={place.project}
            kind={dialog.editor}
            name={dialog.name}
            onClose={close}
          />
        </Suspense>
      ) : null}
    </div>
  )
}

function Folder({
  place,
  files,
  path,
  filter,
  onFilter,
  onOpen,
  onDialog,
}: {
  place: Place
  files: Files
  path: string
  filter: string
  onFilter: (value: string) => void
  onOpen: (path: string | undefined) => void
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  const deferred = useDeferredValue(filter)
  const list = useQuery({
    ...listQuery(place, path, deferred),
    placeholderData: (previous) => previous,
  })
  const mount = mountOf(files.mounts, path)
  const picker = useRef<HTMLInputElement>(null)
  const writable = mount !== undefined && !mount.readOnly
  const why = writable ? undefined : t("files.readOnly")
  const items = (list.data?.items ?? []).filter(
    (e) => !(path === "/" && kernel.includes(`/${e.name}`)),
  )
  const total = list.data?.totalSize ?? 0
  const capped = Boolean(list.data && (list.data.more || items.length < total))
  const actions = (
    <>
      <WriteButton
        why={why}
        label={t("files.newFolder")}
        icon={<FolderPlusIcon />}
        onClick={() => onDialog({ kind: "folder" })}
      />
      <WriteButton
        why={why}
        label={t("files.upload")}
        icon={<UploadSimpleIcon />}
        primary
        onClick={() => picker.current?.click()}
      />
      <input
        ref={picker}
        type="file"
        multiple
        hidden
        onChange={(event) => {
          const chosen = [...(event.target.files ?? [])]
          event.target.value = ""
          if (chosen.length > 0) onDialog({ kind: "upload", files: chosen })
        }}
      />
    </>
  )
  return (
    <Card variant="list">
      <FolderHeader mounts={files.mounts} path={path} onOpen={onOpen} actions={actions} />
      {mount ? <MountNotice mount={mount} onDialog={onDialog} /> : null}
      {total > 12 || filter ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-4 py-2">
          <FilterField value={filter} onChange={onFilter} label={t("files.filter")} />
        </div>
      ) : null}
      {list.isPending ? (
        <div className="p-4">
          <LoadingRows label={t("files.loading")} />
        </div>
      ) : list.isError ? (
        <div className="p-4">
          <FolderProblem
            error={list.error}
            project={place.project}
            retry={() => void list.refetch()}
          />
        </div>
      ) : (
        <EntryTable
          place={place}
          files={files}
          mount={mount}
          path={path}
          entries={items}
          onOpen={onOpen}
          onDialog={onDialog}
        />
      )}
      {list.data && items.length > 0 ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 type-meta text-muted-foreground">
          {capped ? (
            <Capped list={list.data} shown={items.length} />
          ) : (
            <span>{t("files.items", items.length)}</span>
          )}
        </div>
      ) : null}
    </Card>
  )
}

function Capped({ list, shown }: { list?: Schemas["FileList"]; shown: number }) {
  const t = useT()
  if (!list) return null
  return (
    <span className="flex items-center gap-1 type-meta text-muted-foreground">
      {list.more
        ? t("files.shownOfMore", shown, list.totalSize)
        : t("files.shownOf", shown, list.totalSize)}
      <Hint label={t("files.shownOfHint", list.items.length)}>
        {t("files.shownOfHint", list.items.length)}
      </Hint>
    </span>
  )
}

function FolderProblem({
  error,
  project,
  retry,
}: {
  error: unknown
  project: string
  retry: () => void
}) {
  const problem = toProblem(error)
  if (isRefusal(problem)) return <RefusedFiles project={project} problem={problem} retry={retry} />
  return <Problem problem={problem} retry={retry} />
}

function WriteButton({
  why,
  label,
  icon,
  primary,
  onClick,
}: {
  why?: string
  label: string
  icon: ReactNode
  primary?: boolean
  onClick: () => void
}) {
  const button = (
    <Button
      variant={primary ? "default" : "outline"}
      size="sm"
      disabled={Boolean(why)}
      onClick={onClick}
    >
      <span aria-hidden="true" data-icon="inline-start" className="contents">
        {icon}
      </span>
      {label}
    </Button>
  )
  return why ? (
    <Tooltip content={why}>
      <span className="inline-flex">{button}</span>
    </Tooltip>
  ) : (
    button
  )
}

/** One line under the path where the volume changes what the tab may do. */
function MountNotice({ mount, onDialog }: { mount: Mount; onDialog: (dialog: Dialogs) => void }) {
  const t = useT()
  const object = mount.object ?? undefined
  if ((mount.kind === "configMap" || mount.kind === "secret") && object) {
    const secret = mount.kind === "secret"
    return (
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 bg-muted/40 px-4 py-2.5 text-sm">
        <LockSimpleIcon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1">
          {secret ? t("files.secretNotice", object) : t("files.configMapNotice", object)}
        </span>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            onDialog({ kind: "editor", editor: secret ? "Secret" : "ConfigMap", name: object })
          }
        >
          {secret ? (
            <EyeIcon aria-hidden="true" data-icon="inline-start" />
          ) : (
            <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
          )}
          {secret ? t("files.reveal", object) : t("files.edit", object)}
        </Button>
      </div>
    )
  }
  const lost = mount.readOnly
    ? undefined
    : mount.kind === "image"
      ? t("files.imageNotice")
      : mount.lost
        ? t("files.emptyDirNotice")
        : undefined
  if (!lost) return null
  return (
    <div className="px-4 py-2.5">
      <Alert variant="warn" role="note">
        <WarningFillIcon aria-hidden="true" />
        <AlertDescription>{lost}</AlertDescription>
      </Alert>
    </div>
  )
}

function EntryIcon({ entry }: { entry: Entry }) {
  const Icon = entry.type === "directory" ? FolderIcon : entry.link ? LinkSimpleIcon : FileIcon
  return (
    <Icon
      aria-hidden="true"
      className={cn(
        "size-4 shrink-0",
        entry.type === "directory" ? "text-foreground" : "text-muted-foreground",
      )}
    />
  )
}

function EntryTable({
  place,
  files,
  mount,
  path,
  entries,
  onOpen,
  onDialog,
}: {
  place: Place
  files: Files
  mount?: Mount
  path: string
  entries: Entry[]
  onOpen: (path: string) => void
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  const size = useSize()
  const secret = mount?.kind === "secret"
  if (entries.length === 0)
    return <p className="px-4 py-6 text-sm text-muted-foreground">{t("files.empty")}</p>
  return (
    <table className="w-full text-sm max-sm:block sm:table-fixed" aria-label={path}>
      <thead className={thead}>
        <tr>
          <th className={`${head} sm:pl-4`}>{t("files.name")}</th>
          <th className={`${head} w-28 text-right`}>{t("files.size")}</th>
          <th className={`${head} w-36`}>{t("files.modified")}</th>
          <th className={`${head} w-14 sm:pr-4`}>
            <span className="sr-only">{t("files.actions")}</span>
          </th>
        </tr>
      </thead>
      <tbody className="divide-y max-sm:block">
        {entries.map((entry) => {
          const full = join(path, entry.name)
          const directory = entry.type === "directory"
          const file = entry.type === "file"
          const label = (
            <>
              <EntryIcon entry={entry} />
              <span className="truncate font-mono" translate="no">
                {entry.name}
              </span>
            </>
          )
          const open = directory
            ? () => onOpen(full)
            : file && !secret
              ? () => onDialog({ kind: "preview", path: full })
              : undefined
          return (
            <tr key={entry.name} className={row}>
              <td className={`${cell} sm:pl-4 max-sm:w-full`}>
                {open ? (
                  <button type="button" onClick={open} className={opener}>
                    {label}
                  </button>
                ) : (
                  <span className="flex min-w-0 items-center gap-2">{label}</span>
                )}
              </td>
              <td className={`${cell} text-right text-muted-foreground tabular-nums`}>
                {secret ? (
                  <span className="type-meta">{t("files.hidden")}</span>
                ) : entry.size === undefined || entry.size === null || directory ? (
                  "—"
                ) : (
                  size(entry.size)
                )}
              </td>
              <td className={`${cell} text-muted-foreground`}>
                <Ago iso={isoOf(entry.modified)} />
              </td>
              <td className={`${cell} sm:pr-4 text-right max-sm:ml-auto`}>
                <RowMenu
                  place={place}
                  files={files}
                  mount={mount}
                  entry={entry}
                  path={full}
                  onDialog={onDialog}
                />
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

function RowMenu({
  place,
  files,
  mount,
  entry,
  path,
  onDialog,
}: {
  place: Place
  files: Files
  mount?: Mount
  entry: Entry
  path: string
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  if (mount?.kind === "secret") return null
  const file = entry.type === "file"
  const fixed = !mount || mount.readOnly || holdsMount(files.mounts, path)
  if (!file && fixed) return null
  return (
    <Menu>
      <MenuTrigger
        render={
          <Button variant="ghost" size="icon-xs" aria-label={t("files.actionsFor", entry.name)} />
        }
      >
        <DotsThreeIcon aria-hidden="true" />
      </MenuTrigger>
      <MenuContent align="end">
        {file ? (
          <>
            <MenuItem onClick={() => onDialog({ kind: "preview", path })}>
              <EyeIcon aria-hidden="true" />
              {t("files.preview")}
            </MenuItem>
            <MenuItem render={<a href={contentUrl(place, path)} download={entry.name} />}>
              <DownloadSimpleIcon aria-hidden="true" />
              {t("files.download")}
            </MenuItem>
          </>
        ) : null}
        {fixed ? null : (
          <>
            <MenuItem onClick={() => onDialog({ kind: "rename", entry, path })}>
              <PencilSimpleIcon aria-hidden="true" />
              {t("files.rename")}
            </MenuItem>
            <MenuItem onClick={() => onDialog({ kind: "delete", entry, path })}>
              <TrashIcon aria-hidden="true" />
              {t("files.delete")}
            </MenuItem>
          </>
        )}
      </MenuContent>
    </Menu>
  )
}

function DialogBar({ title, mono }: { title: ReactNode; mono?: boolean }) {
  const t = useT()
  return (
    <div className="flex items-center gap-2 border-b px-5 py-3">
      <div className="min-w-0 flex-1 truncate">
        <DialogTitle>
          <span
            className={cn("block truncate", mono && "font-mono")}
            translate={mono ? "no" : undefined}
          >
            {title}
          </span>
        </DialogTitle>
      </div>
      <DialogClose
        render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
      >
        <XIcon aria-hidden="true" />
      </DialogClose>
    </div>
  )
}

/** A text file's first megabyte in a panel; anything else says to download it. */
function Preview({ place, path, onClose }: { place: Place; path: string; onClose: () => void }) {
  const t = useT()
  const size = useSize()
  const preview = useQuery(previewQuery(place, path))
  const data = preview.data
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <DialogBar title={nameOf(path)} mono />
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {preview.isPending ? (
            <LoadingRows label={t("files.loading")} />
          ) : preview.isError ? (
            <Problem error={preview.error} retry={() => void preview.refetch()} />
          ) : data?.tooLarge ? (
            <p className="text-sm text-muted-foreground">{t("files.tooLarge", size(data.size))}</p>
          ) : data?.text === undefined || data.text === null ? (
            <p className="text-sm text-muted-foreground">{t("files.binary")}</p>
          ) : (
            <pre
              className="overflow-x-auto rounded-xl border bg-muted/30 p-3 font-mono text-xs leading-5"
              translate="no"
            >
              {data.text}
            </pre>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t px-5 py-2.5">
          <span className="truncate font-mono type-meta text-muted-foreground" translate="no">
            {path}
          </span>
          {data ? (
            <span className="type-meta text-muted-foreground tabular-nums">{size(data.size)}</span>
          ) : null}
          <Button
            variant="outline"
            size="sm"
            className="ml-auto"
            render={<a href={contentUrl(place, path)} download={nameOf(path)} />}
          >
            <DownloadSimpleIcon aria-hidden="true" data-icon="inline-start" />
            {t("files.download")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function useRefresh(place: Place) {
  const client = useQueryClient()
  return () => client.invalidateQueries({ queryKey: filesKey(place.project, place.pod) })
}

/** A confirmation: what happens, the warning that goes with the mount, and the button that does it. */
function Confirm({
  title,
  children,
  action,
  destructive,
  busy,
  problem,
  onConfirm,
  onClose,
}: {
  title: ReactNode
  children: ReactNode
  action: string
  destructive?: boolean
  busy: boolean
  problem?: ProblemBody
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open || busy ? undefined : onClose())}>
      <DialogContent variant="compact">
        <form
          className="flex min-h-0 flex-1 flex-col"
          onSubmit={(event) => {
            event.preventDefault()
            onConfirm()
          }}
        >
          <DialogBar title={title} />
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 py-4 text-sm">
            {children}
            {problem ? <Problem problem={problem} /> : null}
          </div>
          <div className="flex items-center justify-end gap-2 border-t px-5 py-3">
            <Button type="button" variant="ghost" size="sm" onClick={onClose} disabled={busy}>
              {t("plan.cancel")}
            </Button>
            <Button
              type="submit"
              variant={destructive ? "destructive" : "default"}
              size="sm"
              disabled={busy}
            >
              {action}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** A write answers 204 with no body; only a failure has one. */
function settle(result: { error?: unknown; response: Response }) {
  if (!result.response.ok) unwrap({ ...result, data: undefined })
}

/** Runs a write once; keeps its problem for the dialog to show. */
function useWrite(place: Place, onDone: () => void) {
  const refresh = useRefresh(place)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<ProblemBody>()
  const run = async (write: () => Promise<unknown>) => {
    setBusy(true)
    setProblem(undefined)
    try {
      await write()
      await refresh()
      onDone()
    } catch (error) {
      setProblem(toProblem(error))
    } finally {
      setBusy(false)
    }
  }
  return { busy, problem, run }
}

function LostWarning({ mount }: { mount?: Mount }) {
  const t = useT()
  if (!mount?.lost) return null
  return (
    <Alert variant="warn" role="note">
      <WarningFillIcon aria-hidden="true" />
      <AlertDescription>
        {mount.kind === "image" ? t("files.imageNotice") : t("files.emptyDirNotice")}
      </AlertDescription>
    </Alert>
  )
}

const validName = (name: string) =>
  name !== "" && name !== "." && name !== ".." && !name.includes("/") && !name.includes("\0")

function FolderDialog({
  place,
  files,
  folder,
  onClose,
}: {
  place: Place
  files: Files
  folder: string
  onClose: () => void
}) {
  const t = useT()
  const [name, setName] = useState("")
  const { busy, problem, run } = useWrite(place, onClose)
  const valid = validName(name)
  return (
    <Confirm
      title={t("files.newFolder")}
      action={t("files.create")}
      busy={busy || !valid}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(async () =>
          settle(
            await api.POST("/api/v1/projects/{project}/pods/{pod}/files/folders", {
              params: { path: { project: place.project, pod: place.pod } },
              body: { container: place.container, via: place.via, path: join(folder, name) },
            }),
          ),
        )
      }
    >
      <FormField label={t("files.folderName")}>
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          autoFocus
          spellCheck={false}
        />
      </FormField>
      <p className="text-muted-foreground">
        {t("files.in")}{" "}
        <span className="font-mono" translate="no">
          {folder}
        </span>
      </p>
      <LostWarning mount={mountOf(files.mounts, folder)} />
    </Confirm>
  )
}

function RenameDialog({
  place,
  path,
  onClose,
}: {
  place: Place
  path: string
  onClose: () => void
}) {
  const t = useT()
  const old = nameOf(path)
  const [name, setName] = useState(old)
  const { busy, problem, run } = useWrite(place, onClose)
  const valid = validName(name) && name !== old
  return (
    <Confirm
      title={t("files.renameTitle", old)}
      action={t("files.rename")}
      busy={busy || !valid}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(async () =>
          settle(
            await api.POST("/api/v1/projects/{project}/pods/{pod}/files/moves", {
              params: { path: { project: place.project, pod: place.pod } },
              body: { container: place.container, via: place.via, path, name },
            }),
          ),
        )
      }
    >
      <FormField label={t("files.newName")}>
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          autoFocus
          spellCheck={false}
        />
      </FormField>
      <p className="text-muted-foreground">
        {t("files.in")}{" "}
        <span className="font-mono" translate="no">
          {parentOf(path)}
        </span>
      </p>
    </Confirm>
  )
}

function DeleteDialog({
  place,
  files,
  entry,
  path,
  onClose,
}: {
  place: Place
  files: Files
  entry: Entry
  path: string
  onClose: () => void
}) {
  const t = useT()
  const size = useSize()
  const mount = mountOf(files.mounts, path)
  const { busy, problem, run } = useWrite(place, onClose)
  const directory = entry.type === "directory"
  const what = directory
    ? t("files.deleteFolder", entry.name)
    : t("files.deleteFile", entry.name, entry.size ? size(entry.size) : "")
  return (
    <Confirm
      title={t("files.deleteTitle", entry.name)}
      action={t("files.deleteConfirm", entry.name)}
      destructive
      busy={busy}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(async () =>
          settle(
            await api.DELETE("/api/v1/projects/{project}/pods/{pod}/files/entries", {
              params: {
                path: { project: place.project, pod: place.pod },
                query: { container: place.container, via: place.via, path },
              },
            }),
          ),
        )
      }
    >
      <Alert variant="warn" role="note">
        <WarningFillIcon aria-hidden="true" />
        <AlertDescription>
          {what}{" "}
          {mount?.kind === "claim" && mount.object
            ? t("files.deleteClaim", mount.object)
            : t("files.deleteForever")}
        </AlertDescription>
      </Alert>
      <dl className="grid grid-cols-pairs gap-x-4 gap-y-1">
        <dt className="text-muted-foreground">{t("files.target")}</dt>
        <dd className="font-mono break-all" translate="no">
          {path}
        </dd>
        <dt className="text-muted-foreground">{t("files.replicaLabel")}</dt>
        <dd className="font-mono break-all" translate="no">
          {place.pod}
        </dd>
      </dl>
    </Confirm>
  )
}

interface Upload {
  name: string
  size: number
  done: number
  state: "waiting" | "sending" | "done" | "failed"
  problem?: ProblemBody
}

/** One file, as raw bytes; the browser sets `Content-Length`. */
function send(
  place: Place,
  path: string,
  file: File,
  replace: boolean,
  onProgress: (done: number) => void,
) {
  return new Promise<void>((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open(
      "PUT",
      `${base(place)}/content?${query(place, { path, replace: replace ? "true" : undefined })}`,
    )
    for (const [name, value] of Object.entries(REQUEST_HEADERS))
      request.setRequestHeader(name, value)
    request.upload.onprogress = (event) => onProgress(event.loaded)
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) return resolve()
      let problem: ProblemBody
      try {
        problem = JSON.parse(request.responseText) as ProblemBody
      } catch {
        problem = localProblem(request.status, `The server answered ${request.status}.`)
      }
      reject(new ApiError(request.status, problem))
    }
    request.onerror = () => reject(new ApiError(0, localProblem(0, "The upload was cut off.")))
    request.send(file)
  })
}

function UploadDialog({
  place,
  files,
  folder,
  chosen,
  onClose,
  onProgress,
}: {
  place: Place
  files: Files
  folder: string
  chosen: File[]
  onClose: () => void
  onProgress: (uploads: Upload[]) => void
}) {
  const t = useT()
  const size = useSize()
  const refresh = useRefresh(place)
  const listed = useQuery(listQuery(place, folder, "")).data?.items ?? []
  const existing = new Set(listed.map((e) => e.name))
  const replacing = chosen.filter((f) => existing.has(f.name))
  const tooLarge = chosen.filter((f) => f.size > files.limits.transfer)
  const start = async () => {
    onClose()
    const state: Upload[] = chosen.map((f) => ({
      name: f.name,
      size: f.size,
      done: 0,
      state: "waiting",
    }))
    const update = (index: number, change: Partial<Upload>) => {
      state[index] = { ...state[index]!, ...change }
      onProgress([...state])
    }
    onProgress([...state])
    for (const [index, file] of chosen.entries()) {
      update(index, { state: "sending" })
      try {
        await send(place, join(folder, file.name), file, existing.has(file.name), (done) =>
          update(index, { done }),
        )
        update(index, { state: "done", done: file.size })
      } catch (error) {
        update(index, { state: "failed", problem: toProblem(error) })
      }
    }
    await refresh()
  }
  return (
    <Confirm
      title={t("files.uploadTitle", chosen.length)}
      action={t("files.upload")}
      busy={tooLarge.length > 0}
      onClose={onClose}
      onConfirm={() => void start()}
    >
      <p>
        {t("files.uploadTo")}{" "}
        <span className="font-mono" translate="no">
          {folder}
        </span>
      </p>
      <ul className="flex flex-col divide-y rounded-lg border">
        {chosen.map((file) => (
          <li key={file.name} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate font-mono" translate="no">
              {file.name}
            </span>
            {existing.has(file.name) ? (
              <span className="type-meta text-warn-strong">{t("files.replaces")}</span>
            ) : null}
            <span className="type-meta text-muted-foreground tabular-nums">{size(file.size)}</span>
          </li>
        ))}
      </ul>
      {replacing.length > 0 ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>{t("files.replaceWarning", replacing.length)}</AlertDescription>
        </Alert>
      ) : null}
      {tooLarge.length > 0 ? (
        <Alert variant="destructive" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>
            {t("files.uploadTooLarge", size(files.limits.transfer))}
          </AlertDescription>
        </Alert>
      ) : (
        <p className="type-meta text-muted-foreground">
          {t("files.uploadLimit", size(files.limits.transfer))}
        </p>
      )}
      <LostWarning mount={mountOf(files.mounts, folder)} />
    </Confirm>
  )
}

function UploadProgress({ uploads, onDismiss }: { uploads: Upload[]; onDismiss: () => void }) {
  const t = useT()
  const size = useSize()
  const finished = uploads.every((u) => u.state === "done" || u.state === "failed")
  return (
    <Card variant="list" aria-live="polite">
      <div className="flex min-h-11 items-center gap-2 px-4">
        <UploadSimpleIcon aria-hidden="true" className="size-4 text-muted-foreground" />
        <h2 className="min-w-0 flex-1 truncate text-sm font-medium">
          {finished ? t("files.uploadsDone", uploads.length) : t("files.uploading", uploads.length)}
        </h2>
        {finished ? (
          <Button variant="ghost" size="xs" onClick={onDismiss}>
            {t("common.close")}
          </Button>
        ) : null}
      </div>
      <ul className="flex flex-col divide-y">
        {uploads.map((upload) => {
          const share =
            upload.size === 0 ? (upload.state === "done" ? 1 : 0) : upload.done / upload.size
          return (
            <li key={upload.name} className="flex flex-col gap-1.5 px-4 py-2.5">
              <div className="flex items-center gap-3 text-sm">
                <span className="min-w-0 flex-1 truncate font-mono" translate="no">
                  {upload.name}
                </span>
                <span className="type-meta text-muted-foreground tabular-nums">
                  {upload.state === "done"
                    ? t("files.uploaded")
                    : upload.state === "waiting"
                      ? t("files.waiting")
                      : upload.state === "failed"
                        ? t("files.failed")
                        : t("files.of", size(upload.done), size(upload.size))}
                </span>
              </div>
              {upload.problem ? (
                <Problem problem={upload.problem} />
              ) : (
                <div
                  className="h-1 overflow-hidden rounded-full bg-muted"
                  role="progressbar"
                  aria-label={upload.name}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(share * 100)}
                >
                  <div
                    className={cn(
                      "h-full w-(--share) rounded-full",
                      upload.state === "done" ? "bg-ok" : "bg-progress",
                    )}
                    style={{ "--share": `${share * 100}%` }}
                  />
                </div>
              )}
            </li>
          )
        })}
      </ul>
    </Card>
  )
}
