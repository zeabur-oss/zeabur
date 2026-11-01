import { useRef, useState, type DragEvent, type ReactNode } from "react"
import CheckCircleIcon from "~icons/ph/check-circle-fill"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import ZipIcon from "~icons/ph/file-zip"

import { useI18n, useT } from "@/i18n"
import { formatSize } from "@/lib/format"
import {
  entriesOfDrop,
  filesOfEntries,
  filesOfInput,
  archiveKindOf,
  archiveOfDrop,
  type ArchiveKind,
  type SourceFile,
} from "@/lib/pack/source"
import { cn } from "@/lib/utils"
import type { Packed } from "@/lib/pack/pack"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Spinner } from "@zeabur-oss/design-system/components/spinner"

type FolderState =
  | { phase: "empty" }
  | { phase: "reading" }
  | { phase: "ready"; packed: Packed; name: string }
  | { phase: "failed"; message: string }

const ARCHIVE_PROBLEMS = {
  packed: "upload.problem.archivePacked",
  bomb: "upload.problem.archiveBomb",
  link: "upload.problem.archiveLink",
  invalid: "upload.problem.archiveInvalid",
} as const

/** The folder a person gives, read and packed as soon as they give it. */
export function useFolder() {
  const t = useT()
  const { locale } = useI18n()
  const [state, setState] = useState<FolderState>({ phase: "empty" })
  const fail = (error: unknown): FolderState => {
    const largest = (error as { largest?: { name: string; bytes: number }[] }).largest
    const reason = (error as { reason?: string }).reason
    const limit = (error as { limit?: string }).limit
    if (largest && (limit === "packed" || limit === "unpacked"))
      return {
        phase: "failed",
        message: t(
          limit === "packed" ? "upload.problem.packed" : "upload.problem.unpacked",
          t(
            "common.list",
            largest.map((e) => `${e.name} (${formatSize(e.bytes, locale)})`),
          ),
        ),
      }
    const key = ARCHIVE_PROBLEMS[reason as keyof typeof ARCHIVE_PROBLEMS]
    if (key) return { phase: "failed", message: t(key) }
    return { phase: "failed", message: error instanceof Error ? error.message : String(error) }
  }
  const take = async (files: () => Promise<SourceFile[]>, name: string) => {
    setState({ phase: "reading" })
    try {
      const found = await files()
      const { pack } = await import("@/lib/pack/pack")
      const packed = await pack(found)
      setState(
        packed.files === 0
          ? { phase: "failed", message: t("upload.problem.empty") }
          : { phase: "ready", packed, name },
      )
    } catch (error) {
      setState(fail(error))
    }
  }
  const takeArchive = async (kind: ArchiveKind, archive: Promise<File> | File, name: string) => {
    if (kind === "zip")
      return take(async () => (await import("@/lib/pack/zip")).filesOfZip(await archive), name)
    setState({ phase: "reading" })
    try {
      const { inspectTarGz } = await import("@/lib/pack/targz")
      const packed = await inspectTarGz(await archive)
      setState(
        packed.files === 0
          ? { phase: "failed", message: t("upload.problem.empty") }
          : { phase: "ready", packed, name },
      )
    } catch (error) {
      setState(fail(error))
    }
  }
  return {
    state,
    drop: (transfer: DataTransfer) => {
      const entries = entriesOfDrop(transfer)
      const archive = archiveOfDrop(entries)
      const name = entries.length === 1 ? (entries[0]?.name ?? "") : ""
      return archive
        ? takeArchive(archive.kind, archive.file, withoutArchive(name))
        : take(() => filesOfEntries(entries), name)
    },
    folder: (list: FileList) =>
      take(async () => filesOfInput(list), list[0]?.webkitRelativePath.split("/")[0] ?? ""),
    archive: (file: File) => {
      const kind = archiveKindOf(file.name)
      return kind ? takeArchive(kind, file, withoutArchive(file.name)) : undefined
    },
    clear: () => setState({ phase: "empty" }),
  }
}

const withoutArchive = (name: string) => name.replace(/\.(zip|tar\.gz|tgz)$/i, "")

export type Folder = ReturnType<typeof useFolder>

/** Where an archive or a folder is dropped, or chosen with the file dialog. */
export function FolderDrop({ folder, missing }: { folder: Folder; missing?: boolean }) {
  const t = useT()
  const [over, setOver] = useState(false)
  const { state } = folder
  const onDrop = (event: DragEvent) => {
    event.preventDefault()
    setOver(false)
    void folder.drop(event.dataTransfer)
  }
  return (
    <div className="flex flex-col gap-2">
      <div
        role="group"
        aria-label={t("upload.zone")}
        data-over={over || undefined}
        onDragOver={(event) => {
          event.preventDefault()
          setOver(true)
        }}
        onDragLeave={() => setOver(false)}
        onDrop={onDrop}
        className={cn(
          "flex flex-col items-center gap-3 rounded-lg border border-dashed px-4 py-8 text-center transition-colors motion-reduce:transition-none",
          over && "border-brand bg-accent",
          missing && "border-fail",
        )}
      >
        <DropBody folder={folder} over={over} />
      </div>
      {state.phase === "failed" ? (
        <Alert variant="destructive">
          <AlertDescription>{state.message}</AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}

function DropBody({ folder, over }: { folder: Folder; over: boolean }) {
  const t = useT()
  const { locale } = useI18n()
  const { state } = folder
  if (state.phase === "reading")
    return (
      <p className="flex items-center gap-2 text-sm text-secondary" role="status">
        <Spinner />
        {t("upload.reading")}
      </p>
    )
  if (state.phase === "ready")
    return (
      <>
        <CheckCircleIcon aria-hidden="true" className="size-6 text-ok" />
        <div className="flex flex-col gap-0.5">
          <p className="text-sm font-medium">
            {t("upload.ready", state.packed.files, formatSize(state.packed.blob.size, locale))}
          </p>
          {state.packed.left > 0 ? (
            <p className="type-meta text-muted-foreground">{t("upload.left", state.packed.left)}</p>
          ) : null}
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={folder.clear}>
          {t("upload.change")}
        </Button>
      </>
    )
  return (
    <>
      <ZipIcon aria-hidden="true" className="size-6 text-secondary" />
      <div className="flex max-w-prose flex-col gap-1">
        <p className="text-sm">{over ? t("upload.dropActive") : t("upload.drop")}</p>
        <p className="type-meta text-muted-foreground">{t("upload.dropNote")}</p>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        <Picker
          label={t("upload.chooseArchive")}
          icon={<ZipIcon aria-hidden="true" />}
          onFiles={(list) => void (list[0] && folder.archive(list[0]))}
        />
        <Picker
          label={t("upload.choose")}
          icon={<FolderSimpleIcon aria-hidden="true" />}
          directory
          onFiles={folder.folder}
        />
      </div>
    </>
  )
}

/** A button that opens the file dialog for a folder or a .zip file. */
function Picker({
  label,
  icon,
  directory,
  onFiles,
}: {
  label: string
  icon: ReactNode
  directory?: boolean
  onFiles: (list: FileList) => void
}) {
  const input = useRef<HTMLInputElement>(null)
  return (
    <>
      <Button type="button" variant="outline" size="sm" onClick={() => input.current?.click()}>
        {icon}
        {label}
      </Button>
      <input
        ref={(element) => {
          input.current = element
          if (directory) element?.setAttribute("webkitdirectory", "")
        }}
        type="file"
        hidden
        accept={directory ? undefined : ".zip,.tar.gz,.tgz,application/zip,application/gzip"}
        onChange={(event) => {
          if (event.target.files?.length) onFiles(event.target.files)
          event.target.value = ""
        }}
      />
    </>
  )
}
