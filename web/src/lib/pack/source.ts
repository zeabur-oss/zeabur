// The files a person gives: a dropped folder or archive, or a folder picked with the file dialog.
export interface SourceFile {
  /** Relative to the folder, with `/`. */
  path: string
  data: Blob
}

/** Files of the folder named by the first path segment, without that segment. */
export function withoutRoot(files: SourceFile[]): SourceFile[] {
  const roots = new Set(files.map((f) => f.path.split("/")[0]))
  if (roots.size !== 1 || files.some((f) => !f.path.includes("/"))) return files
  return files.map((f) => ({ ...f, path: f.path.slice(f.path.indexOf("/") + 1) }))
}

/** A folder from `<input webkitdirectory>`: paths begin with the folder's name. */
export function filesOfInput(list: FileList): SourceFile[] {
  return withoutRoot(
    [...list].map((file) => ({ path: file.webkitRelativePath || file.name, data: file })),
  )
}

type Entry = FileSystemEntry

const readBatch = (reader: FileSystemDirectoryReader) =>
  new Promise<Entry[]>((resolve, reject) => reader.readEntries(resolve, reject))

const fileOf = (entry: FileSystemFileEntry) =>
  new Promise<File>((resolve, reject) => entry.file(resolve, reject))

async function walk(entry: Entry, out: SourceFile[]): Promise<void> {
  if (entry.isFile) {
    out.push({
      path: entry.fullPath.replace(/^\/+/, ""),
      data: await fileOf(entry as FileSystemFileEntry),
    })
    return
  }
  const reader = (entry as FileSystemDirectoryEntry).createReader()
  // A reader gives at most 100 entries per call, until one call gives none.
  for (let batch = await readBatch(reader); batch.length > 0; batch = await readBatch(reader))
    for (const child of batch) await walk(child, out)
}

/** What was dropped, taken at once: the browser empties the transfer after the first await. */
export function entriesOfDrop(transfer: DataTransfer): Entry[] {
  return [...transfer.items]
    .map((item) => item.webkitGetAsEntry())
    .filter((entry): entry is Entry => entry !== null)
}

export type ArchiveKind = "zip" | "targz"

/** Which archive a file name says it is. */
export function archiveKindOf(name: string): ArchiveKind | undefined {
  if (/\.zip$/i.test(name)) return "zip"
  return /\.(tar\.gz|tgz)$/i.test(name) ? "targz" : undefined
}

/** The archive alone in a drop, if that is what it holds. */
export function archiveOfDrop(
  entries: Entry[],
): { kind: ArchiveKind; file: Promise<File> } | undefined {
  const only = entries.length === 1 ? entries[0] : undefined
  const kind = only?.isFile ? archiveKindOf(only.name) : undefined
  return only && kind ? { kind, file: fileOf(only as FileSystemFileEntry) } : undefined
}

/** The files of a dropped folder, or of several dropped files and folders as one folder. */
export async function filesOfEntries(entries: Entry[]): Promise<SourceFile[]> {
  const out: SourceFile[] = []
  for (const entry of entries) await walk(entry, out)
  const only = entries.length === 1 ? entries[0] : undefined
  return only?.isDirectory ? withoutRoot(out) : out
}
