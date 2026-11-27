// An object store's Files tab: one bucket's folders and objects, read and changed through the
// store's S3 gateway by Zeabur, which holds the keys.
import { useInfiniteQuery, useQuery } from "@tanstack/react-query"
import { useState, type ReactNode } from "react"
import ArrowUpIcon from "~icons/ph/arrow-up"
import CaretRightIcon from "~icons/ph/caret-right"
import DotsThreeIcon from "~icons/ph/dots-three"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import EyeIcon from "~icons/ph/eye"
import FileIcon from "~icons/ph/file"
import FolderIcon from "~icons/ph/folder"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import TrashIcon from "~icons/ph/trash"
import WarningFillIcon from "~icons/ph/warning-fill"

import {
  bucketPreviewQuery,
  countQuery,
  listingQuery,
  objectUrl,
  refreshBucket,
  type BucketPlace,
} from "@/api/bucket-files"
import { toProblem, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { Ago } from "@/components/bits"
import {
  cell,
  ChosenFiles,
  Confirm,
  DialogBar,
  EntryTableHead,
  opener,
  putBytes,
  row,
  sendAll,
  UploadProgress,
  useSize,
  useWrite,
  WriteActions,
  type Upload,
} from "@/components/file-manager"
import { FormField } from "@/components/form-field"
import { useGate, useGateReason } from "@/components/gate"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { isRefusal, Refused } from "@/components/refused"
import { useT } from "@/i18n"
import type { ObjectStorage } from "@/model/object-storage"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card } from "@zeabur-oss/design-system/components/card"
import { Dialog, DialogContent } from "@zeabur-oss/design-system/components/dialog"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
} from "@zeabur-oss/design-system/components/empty"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { Select } from "@zeabur-oss/design-system/components/select"

type BucketObject = Schemas.BucketObject

/** The part sent at once: small enough to show progress, well under the server's limit. */
const PART = 16 * 1024 * 1024

/** The last segment of a key or of a folder's prefix, without its `/`. */
const nameOf = (key: string) => key.replace(/\/$/, "").split("/").pop() ?? key
const parentOf = (prefix: string) => prefix.replace(/[^/]*\/$/, "")
const validName = (name: string) =>
  name !== "" && !name.includes("/") && name !== "." && name !== ".."

export function BucketFilesTab({
  project,
  store,
  bucket: asked,
  prefix,
  onOpen,
  onOverview,
}: {
  project: string
  store: ObjectStorage
  bucket?: string
  prefix?: string
  onOpen: (bucket: string, prefix?: string) => void
  onOverview: () => void
}) {
  const t = useT()
  const bucket = store.buckets.find((b) => b.s3Name === asked) ?? store.buckets[0]
  if (!bucket) return <NoBuckets onOverview={onOverview} />
  const place: BucketPlace = {
    project,
    store: store.name,
    bucket: bucket.s3Name,
  }
  return (
    <div className="flex flex-col gap-4">
      {store.buckets.length > 1 ? (
        <Select
          size="sm"
          className="self-start"
          value={bucket.s3Name}
          items={store.buckets.map((b) => ({
            value: b.s3Name,
            label: b.s3Name,
          }))}
          onValueChange={(value) => onOpen(value)}
          aria-label={t("bucketFiles.bucket")}
          translate="no"
        />
      ) : null}
      <Browser
        key={bucket.s3Name}
        place={place}
        prefix={prefix ?? ""}
        onOpen={(next) => onOpen(bucket.s3Name, next || undefined)}
      />
    </div>
  )
}

function NoBuckets({ onOverview }: { onOverview: () => void }) {
  const t = useT()
  return (
    <Empty size="sm">
      <EmptyHeader>
        <EmptyDescription>{t("bucketFiles.noBuckets")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" size="sm" onClick={onOverview}>
          {t("bucketFiles.openOverview")}
        </Button>
      </EmptyContent>
    </Empty>
  )
}

type Dialogs =
  | { kind: "preview"; object: BucketObject }
  | { kind: "rename"; object: BucketObject }
  | { kind: "deleteObject"; object: BucketObject }
  | { kind: "deleteFolder"; folder: string }
  | { kind: "folder" }
  | { kind: "upload"; files: File[] }

function Browser({
  place,
  prefix,
  onOpen,
}: {
  place: BucketPlace
  prefix: string
  onOpen: (prefix: string) => void
}) {
  const list = useInfiniteQuery(listingQuery(place, prefix))
  const [dialog, setDialog] = useState<Dialogs>()
  const [uploads, setUploads] = useState<Upload[]>([])
  const pages = list.data?.pages ?? []
  const objects = pages.flatMap((p) => p.objects)
  return (
    <div className="flex min-w-0 flex-col gap-3">
      {uploads.length > 0 ? (
        <UploadProgress uploads={uploads} onDismiss={() => setUploads([])} />
      ) : null}
      <Card variant="list">
        <Crumbs place={place} prefix={prefix} onOpen={onOpen} onDialog={setDialog} />
        <Listing
          list={list}
          place={place}
          prefix={prefix}
          folders={pages.flatMap((p) => p.folders)}
          objects={objects}
          onOpen={onOpen}
          onDialog={setDialog}
        />
      </Card>
      <Dialogs
        place={place}
        prefix={prefix}
        dialog={dialog}
        objects={objects}
        limits={pages[0]?.limits}
        onClose={() => setDialog(undefined)}
        onProgress={setUploads}
      />
    </div>
  )
}

function Crumbs({
  place,
  prefix,
  onOpen,
  onDialog,
}: {
  place: BucketPlace
  prefix: string
  onOpen: (prefix: string) => void
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  const parts = prefix.split("/").filter(Boolean)
  const crumbs = [
    { prefix: "", label: place.bucket },
    ...parts.map((part, index) => ({
      prefix: `${parts.slice(0, index + 1).join("/")}/`,
      label: part,
    })),
  ]
  return (
    <div className="flex min-h-11 flex-wrap items-center gap-x-2 gap-y-1.5 px-3 py-1.5">
      {prefix ? (
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label={t("files.up")}
          onClick={() => onOpen(parentOf(prefix))}
        >
          <ArrowUpIcon aria-hidden="true" />
        </Button>
      ) : null}
      <nav aria-label={t("files.location")} className="min-w-0 flex-1">
        <ol
          className="flex min-w-0 flex-wrap items-center gap-0.5 font-mono text-sm"
          translate="no"
        >
          {crumbs.map((crumb, index) => (
            <Crumb
              key={crumb.prefix}
              label={crumb.label}
              last={index === crumbs.length - 1}
              onOpen={() => onOpen(crumb.prefix)}
            />
          ))}
        </ol>
      </nav>
      <BucketActions place={place} onDialog={onDialog} />
    </div>
  )
}

function Crumb({ label, last, onOpen }: { label: string; last: boolean; onOpen: () => void }) {
  if (last)
    return (
      <li className="flex min-w-0 items-center">
        <span aria-current="location" className="truncate px-2 font-medium">
          {label}
        </span>
      </li>
    )
  return (
    <li className="flex min-w-0 items-center gap-0.5">
      <Button variant="ghost" size="xs" onClick={onOpen}>
        {label}
      </Button>
      <CaretRightIcon aria-hidden="true" className="size-3 text-muted-foreground" />
    </li>
  )
}

function BucketActions({
  place,
  onDialog,
}: {
  place: BucketPlace
  onDialog: (dialog: Dialogs) => void
}) {
  const why = useGateReason()(useGate(place.project, "changeObjectStorage"))
  return (
    <div className="flex items-center gap-1.5 max-sm:w-full">
      <WriteActions
        why={why}
        onFolder={() => onDialog({ kind: "folder" })}
        onFiles={(files) => onDialog({ kind: "upload", files })}
      />
    </div>
  )
}

function Listing({
  list,
  place,
  prefix,
  folders,
  objects,
  onOpen,
  onDialog,
}: {
  list: ReturnType<typeof useInfiniteQuery<Schemas.BucketListing>>
  place: BucketPlace
  prefix: string
  folders: string[]
  objects: BucketObject[]
  onOpen: (prefix: string) => void
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  if (list.isPending)
    return (
      <div className="p-4">
        <LoadingRows label={t("files.loading")} />
      </div>
    )
  if (list.isError)
    return (
      <div className="p-4">
        <ListProblem error={list.error} bucket={place.bucket} />
      </div>
    )
  if (folders.length + objects.length === 0)
    return (
      <p className="px-4 py-6 text-sm text-muted-foreground">
        {prefix ? t("files.empty") : t("bucketFiles.emptyBucket")}
      </p>
    )
  return (
    <>
      <ObjectTable
        place={place}
        prefix={prefix}
        folders={folders}
        objects={objects}
        onOpen={onOpen}
        onDialog={onDialog}
      />
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 type-meta text-muted-foreground">
        <span>{t("files.items", folders.length + objects.length)}</span>
        {list.hasNextPage ? (
          <Button
            variant="ghost"
            size="xs"
            disabled={list.isFetchingNextPage}
            onClick={() => void list.fetchNextPage()}
          >
            {t("bucketFiles.more")}
          </Button>
        ) : null}
      </div>
    </>
  )
}

function ListProblem({ error, bucket }: { error: unknown; bucket: string }) {
  const t = useT()
  const problem = toProblem(error)
  if (isRefusal(problem))
    return <Refused refusal={problem} what={t("bucketFiles.refusedWhat", bucket)} />
  return <Problem problem={problem} />
}

function ObjectTable({
  place,
  prefix,
  folders,
  objects,
  onOpen,
  onDialog,
}: {
  place: BucketPlace
  prefix: string
  folders: string[]
  objects: BucketObject[]
  onOpen: (prefix: string) => void
  onDialog: (dialog: Dialogs) => void
}) {
  return (
    <table
      className="w-full text-sm max-sm:block sm:table-fixed"
      aria-label={prefix || place.bucket}
    >
      <EntryTableHead />
      <tbody className="divide-y max-sm:block">
        {folders.map((folder) => (
          <FolderRow
            key={folder}
            place={place}
            folder={folder}
            onOpen={() => onOpen(folder)}
            onDelete={() => onDialog({ kind: "deleteFolder", folder })}
          />
        ))}
        {objects.map((object) => (
          <ObjectRow key={object.key} place={place} object={object} onDialog={onDialog} />
        ))}
      </tbody>
    </table>
  )
}

function Name({ icon, name }: { icon: ReactNode; name: string }) {
  return (
    <>
      {icon}
      <span className="truncate font-mono" translate="no">
        {name}
      </span>
    </>
  )
}

function FolderRow({
  place,
  folder,
  onOpen,
  onDelete,
}: {
  place: BucketPlace
  folder: string
  onOpen: () => void
  onDelete: () => void
}) {
  const t = useT()
  const gate = useGate(place.project, "changeObjectStorage")
  const name = nameOf(folder)
  return (
    <tr className={row}>
      <td className={`${cell} sm:pl-4 max-sm:w-full`}>
        <button type="button" onClick={onOpen} className={opener}>
          <Name icon={<FolderIcon aria-hidden="true" className="size-4 shrink-0" />} name={name} />
        </button>
      </td>
      <td className={`${cell} text-right text-muted-foreground max-sm:hidden`}>—</td>
      <td className={`${cell} text-muted-foreground max-sm:hidden`}>—</td>
      <td className={`${cell} sm:pr-4 text-right max-sm:ml-auto`}>
        {gate.open ? (
          <Menu>
            <MenuTrigger
              render={
                <Button variant="ghost" size="icon-xs" aria-label={t("files.actionsFor", name)} />
              }
            >
              <DotsThreeIcon aria-hidden="true" />
            </MenuTrigger>
            <MenuContent align="end">
              <MenuItem onClick={onDelete}>
                <TrashIcon aria-hidden="true" />
                {t("files.delete")}
              </MenuItem>
            </MenuContent>
          </Menu>
        ) : null}
      </td>
    </tr>
  )
}

function ObjectRow({
  place,
  object,
  onDialog,
}: {
  place: BucketPlace
  object: BucketObject
  onDialog: (dialog: Dialogs) => void
}) {
  const size = useSize()
  return (
    <tr className={row}>
      <td className={`${cell} sm:pl-4 max-sm:w-full`}>
        <button
          type="button"
          onClick={() => onDialog({ kind: "preview", object })}
          className={opener}
        >
          <Name
            icon={<FileIcon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />}
            name={nameOf(object.key)}
          />
        </button>
      </td>
      <td className={`${cell} text-right text-muted-foreground tabular-nums`}>
        {size(object.size)}
      </td>
      <td className={`${cell} text-muted-foreground`}>
        {object.lastModified ? <Ago iso={object.lastModified} /> : "—"}
      </td>
      <td className={`${cell} sm:pr-4 text-right max-sm:ml-auto`}>
        <ObjectMenu place={place} object={object} onDialog={onDialog} />
      </td>
    </tr>
  )
}

function ObjectMenu({
  place,
  object,
  onDialog,
}: {
  place: BucketPlace
  object: BucketObject
  onDialog: (dialog: Dialogs) => void
}) {
  const t = useT()
  const gate = useGate(place.project, "changeObjectStorage")
  const name = nameOf(object.key)
  return (
    <Menu>
      <MenuTrigger
        render={<Button variant="ghost" size="icon-xs" aria-label={t("files.actionsFor", name)} />}
      >
        <DotsThreeIcon aria-hidden="true" />
      </MenuTrigger>
      <MenuContent align="end">
        <MenuItem onClick={() => onDialog({ kind: "preview", object })}>
          <EyeIcon aria-hidden="true" />
          {t("files.preview")}
        </MenuItem>
        <MenuItem render={<a href={objectUrl(place, object.key)} download={name} />}>
          <DownloadSimpleIcon aria-hidden="true" />
          {t("files.download")}
        </MenuItem>
        {gate.open ? (
          <>
            <MenuItem onClick={() => onDialog({ kind: "rename", object })}>
              <PencilSimpleIcon aria-hidden="true" />
              {t("files.rename")}
            </MenuItem>
            <MenuItem onClick={() => onDialog({ kind: "deleteObject", object })}>
              <TrashIcon aria-hidden="true" />
              {t("files.delete")}
            </MenuItem>
          </>
        ) : null}
      </MenuContent>
    </Menu>
  )
}

function Dialogs({
  place,
  prefix,
  dialog,
  objects,
  limits,
  onClose,
  onProgress,
}: {
  place: BucketPlace
  prefix: string
  dialog?: Dialogs
  objects: BucketObject[]
  limits?: Schemas.BucketLimits
  onClose: () => void
  onProgress: (uploads: Upload[]) => void
}) {
  if (dialog?.kind === "preview")
    return <Preview place={place} object={dialog.object} onClose={onClose} />
  if (dialog?.kind === "upload")
    return (
      <UploadDialog
        place={place}
        prefix={prefix}
        chosen={dialog.files}
        objects={objects}
        part={Math.min(PART, limits?.part ?? PART)}
        onClose={onClose}
        onProgress={onProgress}
      />
    )
  return dialog ? (
    <ChangeDialog place={place} prefix={prefix} dialog={dialog} onClose={onClose} />
  ) : null
}

function ChangeDialog({
  place,
  prefix,
  dialog,
  onClose,
}: {
  place: BucketPlace
  prefix: string
  dialog: Exclude<Dialogs, { kind: "preview" | "upload" }>
  onClose: () => void
}) {
  switch (dialog.kind) {
    case "rename":
      return <RenameDialog place={place} object={dialog.object} onClose={onClose} />
    case "deleteObject":
      return <DeleteObjectDialog place={place} object={dialog.object} onClose={onClose} />
    case "deleteFolder":
      return <DeleteFolderDialog place={place} folder={dialog.folder} onClose={onClose} />
    case "folder":
      return <FolderDialog place={place} prefix={prefix} onClose={onClose} />
  }
}

/** A text object's first megabyte, or an image, in a panel; anything else says to download it. */
function Preview({
  place,
  object,
  onClose,
}: {
  place: BucketPlace
  object: BucketObject
  onClose: () => void
}) {
  const t = useT()
  const size = useSize()
  const name = nameOf(object.key)
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <DialogBar title={name} mono />
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          <PreviewBody place={place} object={object} />
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t px-6 py-2.5">
          <span className="truncate font-mono type-meta text-muted-foreground" translate="no">
            {object.key}
          </span>
          <span className="type-meta text-muted-foreground tabular-nums">{size(object.size)}</span>
          <Button
            variant="outline"
            size="sm"
            className="ml-auto"
            render={<a href={objectUrl(place, object.key)} download={name} />}
          >
            <DownloadSimpleIcon aria-hidden="true" data-icon="inline-start" />
            {t("files.download")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function PreviewBody({ place, object }: { place: BucketPlace; object: BucketObject }) {
  const t = useT()
  const size = useSize()
  const preview = useQuery(bucketPreviewQuery(place, object.key))
  if (preview.isPending) return <LoadingRows label={t("files.loading")} />
  if (preview.isError) return <Problem error={preview.error} />
  const data = preview.data
  if (data.image)
    return (
      <img
        src={objectUrl(place, object.key, true)}
        alt={nameOf(object.key)}
        className="mx-auto max-h-full max-w-full rounded-lg border"
      />
    )
  if (data.tooLarge)
    return <p className="text-sm text-muted-foreground">{t("files.tooLarge", size(data.size))}</p>
  if (data.text === undefined || data.text === null)
    return <p className="text-sm text-muted-foreground">{t("files.binary")}</p>
  return (
    <pre
      className="overflow-x-auto rounded-xl border bg-muted/30 p-3 font-mono text-xs leading-5"
      translate="no"
    >
      {data.text}
    </pre>
  )
}

/** Runs a write once, then reads the bucket again. */
const useBucketWrite = (place: BucketPlace, onDone: () => void) =>
  useWrite(() => refreshBucket(place), onDone)

function Where({ prefix, bucket }: { prefix: string; bucket: string }) {
  const t = useT()
  return (
    <p className="text-muted-foreground">
      {t("files.in")}{" "}
      <span className="font-mono" translate="no">
        {prefix ? `${bucket}/${prefix}` : bucket}
      </span>
    </p>
  )
}

function NameInput({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <FormField label={label}>
      <Input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        autoFocus
        spellCheck={false}
      />
    </FormField>
  )
}

function FolderDialog({
  place,
  prefix,
  onClose,
}: {
  place: BucketPlace
  prefix: string
  onClose: () => void
}) {
  const t = useT()
  const [name, setName] = useState("")
  const { busy, problem, run } = useBucketWrite(place, onClose)
  return (
    <Confirm
      title={t("files.newFolder")}
      action={t("files.create")}
      busy={busy || !validName(name)}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(
          async () =>
            await zeabur.makeBucketFolder(place.project, place.store, place.bucket, {
              prefix: `${prefix}${name}/`,
            }),
        )
      }
    >
      <NameInput label={t("files.folderName")} value={name} onChange={setName} />
      <Where prefix={prefix} bucket={place.bucket} />
    </Confirm>
  )
}

function RenameDialog({
  place,
  object,
  onClose,
}: {
  place: BucketPlace
  object: BucketObject
  onClose: () => void
}) {
  const t = useT()
  const old = nameOf(object.key)
  const prefix = object.key.slice(0, object.key.length - old.length)
  const [name, setName] = useState(old)
  const { busy, problem, run } = useBucketWrite(place, onClose)
  return (
    <Confirm
      title={t("files.renameTitle", old)}
      action={t("files.rename")}
      busy={busy || !validName(name) || name === old}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(
          async () =>
            await zeabur.renameBucketObject(place.project, place.store, place.bucket, {
              from: object.key,
              to: `${prefix}${name}`,
            }),
        )
      }
    >
      <NameInput label={t("files.newName")} value={name} onChange={setName} />
      <Where prefix={prefix} bucket={place.bucket} />
      <p className="type-meta text-muted-foreground">{t("bucketFiles.renameCopies")}</p>
    </Confirm>
  )
}

function DeleteWarning({ what }: { what: string }) {
  const t = useT()
  return (
    <Alert variant="warn" role="note">
      <WarningFillIcon aria-hidden="true" />
      <AlertDescription>{t("common.sentences", [what, t("files.deleteForever")])}</AlertDescription>
    </Alert>
  )
}

function DeleteObjectDialog({
  place,
  object,
  onClose,
}: {
  place: BucketPlace
  object: BucketObject
  onClose: () => void
}) {
  const t = useT()
  const size = useSize()
  const name = nameOf(object.key)
  const { busy, problem, run } = useBucketWrite(place, onClose)
  return (
    <Confirm
      title={t("files.deleteTitle", name)}
      action={t("files.deleteConfirm", name)}
      destructive
      busy={busy}
      problem={problem}
      onClose={onClose}
      onConfirm={() =>
        void run(
          async () =>
            await zeabur.deleteBucketObjects(place.project, place.store, place.bucket, {
              key: object.key,
            }),
        )
      }
    >
      <DeleteWarning what={t("files.deleteFile", name, size(object.size))} />
      <dl className="grid grid-cols-pairs gap-x-4 gap-y-1">
        <dt className="text-muted-foreground">{t("bucketFiles.key")}</dt>
        <dd className="font-mono break-all" translate="no">
          {object.key}
        </dd>
      </dl>
    </Confirm>
  )
}

/** Counts what is under the folder first, so the person knows what goes. */
function DeleteFolderDialog({
  place,
  folder,
  onClose,
}: {
  place: BucketPlace
  folder: string
  onClose: () => void
}) {
  const t = useT()
  const size = useSize()
  const name = nameOf(folder)
  const count = useQuery(countQuery(place, folder))
  const { busy, problem, run } = useBucketWrite(place, onClose)
  const data = count.data
  const what = data?.more
    ? t("bucketFiles.deleteFolderMore", name, data.objects)
    : data
      ? t("bucketFiles.deleteFolder", name, data.objects, size(data.bytes))
      : undefined
  return (
    <Confirm
      title={t("files.deleteTitle", name)}
      action={t("files.deleteConfirm", name)}
      destructive
      busy={busy || !data}
      problem={problem ?? (count.isError ? toProblem(count.error) : undefined)}
      onClose={onClose}
      onConfirm={() =>
        void run(
          async () =>
            await zeabur.deleteBucketObjects(place.project, place.store, place.bucket, {
              prefix: folder,
            }),
        )
      }
    >
      {what ? (
        <DeleteWarning what={what} />
      ) : count.isPending ? (
        <LoadingRows label={t("bucketFiles.counting")} />
      ) : null}
      <dl className="grid grid-cols-pairs gap-x-4 gap-y-1">
        <dt className="text-muted-foreground">{t("bucketFiles.prefix")}</dt>
        <dd className="font-mono break-all" translate="no">
          {folder}
        </dd>
      </dl>
    </Confirm>
  )
}

/** One file: whole when it fits in a part, else in parts, aborted when one fails. */
async function uploadFile(
  place: BucketPlace,
  key: string,
  file: File,
  part: number,
  onProgress: (done: number) => void,
) {
  const { project, store, bucket } = place
  if (file.size <= part) {
    await putBytes(
      zeabur.getUploadBucketObjectUrl(project, store, bucket, { key }),
      file,
      onProgress,
    )
    return
  }
  const { upload } = await zeabur.startBucketUpload(project, store, bucket, {
    key,
  })
  try {
    const parts: Schemas.BucketPart[] = []
    for (let at = 0; at < file.size; at += part) {
      const url = zeabur.getSendBucketUploadPartUrl(
        project,
        store,
        bucket,
        upload,
        parts.length + 1,
        {
          key,
        },
      )
      const answer = await putBytes(url, file.slice(at, at + part), (done) => onProgress(at + done))
      parts.push(JSON.parse(answer) as Schemas.BucketPart)
    }
    await zeabur.completeBucketUpload(project, store, bucket, upload, { parts }, { key })
  } catch (error) {
    await zeabur.abortBucketUpload(project, store, bucket, upload, { key }).catch(() => undefined)
    throw error
  }
}

function UploadDialog({
  place,
  prefix,
  chosen,
  objects,
  part,
  onClose,
  onProgress,
}: {
  place: BucketPlace
  prefix: string
  chosen: File[]
  objects: BucketObject[]
  part: number
  onClose: () => void
  onProgress: (uploads: Upload[]) => void
}) {
  const t = useT()
  const size = useSize()
  const existing = new Set(objects.map((o) => nameOf(o.key)))
  const start = async () => {
    onClose()
    await sendAll(
      chosen,
      (file, progress) => uploadFile(place, `${prefix}${file.name}`, file, part, progress),
      onProgress,
    )
    await refreshBucket(place)
  }
  return (
    <Confirm
      title={t("files.uploadTitle", chosen.length)}
      action={t("files.upload")}
      busy={false}
      onClose={onClose}
      onConfirm={() => void start()}
    >
      <Where prefix={prefix} bucket={place.bucket} />
      <ChosenFiles chosen={chosen} existing={existing} />
      <p className="type-meta text-muted-foreground">{t("bucketFiles.parts", size(part))}</p>
    </Confirm>
  )
}
