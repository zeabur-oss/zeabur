// Packs a folder as the gzip tar Zeabur Builder builds (docs/archive-source.md in its repository):
// what `.gitignore` and `.dockerignore` leave in, without `.git`. A browser does not tell a file's
// mode, so a file that starts with `#!` is made executable; a zip's own modes and links are kept.
import { excluder, isIgnoreFile, type IgnoreFile } from "@/lib/pack/ignore"
import { MAX_PACKED, MAX_UNPACKED } from "@/lib/pack/limits"
import type { SourceFile } from "@/lib/pack/source"
import { tarParts } from "@/lib/pack/tar"

export interface Packed {
  blob: Blob
  /** Files in the archive. */
  files: number
  /** Files an ignore file or `.git` left out. */
  left: number
  /** The files together, unpacked. */
  bytes: number
}

/** The folder is past a limit; the largest top-level entries, to leave out. */
export class TooLarge extends Error {
  readonly limit: "packed" | "unpacked"
  readonly largest: { name: string; bytes: number }[]

  constructor(limit: "packed" | "unpacked", largest: { name: string; bytes: number }[]) {
    super(limit)
    this.limit = limit
    this.largest = largest
  }
}

async function startsWithShebang(file: Blob): Promise<boolean> {
  return file.size >= 2 && (await file.slice(0, 2).text()) === "#!"
}

function largestEntries(files: readonly SourceFile[]) {
  const sizes = new Map<string, number>()
  for (const file of files) {
    const top = file.path.split("/")[0] ?? file.path
    sizes.set(top, (sizes.get(top) ?? 0) + file.data.size)
  }
  const total = [...sizes.values()].reduce((sum, bytes) => sum + bytes, 0)
  return [...sizes]
    .map(([name, bytes]) => ({ name, bytes }))
    .sort((a, b) => b.bytes - a.bytes)
    .slice(0, 3)
    .filter((entry, i) => i === 0 || entry.bytes >= total / 10)
}

async function gzip(parts: BlobPart[]): Promise<Blob> {
  const stream = new Blob(parts).stream().pipeThrough(new CompressionStream("gzip"))
  return new Response(stream).blob()
}

export async function pack(files: readonly SourceFile[]): Promise<Packed> {
  const ignoreFiles: IgnoreFile[] = []
  for (const file of files) {
    const kind = isIgnoreFile(file.path)
    if (kind)
      ignoreFiles.push({
        folder: file.path.includes("/") ? file.path.slice(0, file.path.lastIndexOf("/")) : "",
        kind,
        text: await file.data.text(),
      })
  }
  const leaves = excluder(ignoreFiles)
  const kept = files.filter((file) => !leaves.file(file.path))
  const bytes = kept.reduce((total, file) => total + file.data.size, 0)
  if (bytes > MAX_UNPACKED) throw new TooLarge("unpacked", largestEntries(kept))
  const tarFiles = []
  for (const file of kept)
    tarFiles.push({
      path: file.path,
      data: file.data,
      executable: file.executable ?? (await startsWithShebang(file.data)),
      link: file.link,
    })
  const blob = await gzip(tarParts(tarFiles))
  if (blob.size > MAX_PACKED) throw new TooLarge("packed", largestEntries(kept))
  return { blob, files: kept.length, left: files.length - kept.length, bytes }
}
