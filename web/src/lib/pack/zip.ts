// The files of a zip. Apart from source.ts so that the unzip code loads only for a zip.
import { unzipSync } from "fflate"

import { MAX_UNPACKED } from "@/lib/pack/limits"
import { withoutRoot, type SourceFile } from "@/lib/pack/source"

/** The zip would unpack to more than the limit, is not a zip, or holds a link out of the folder. */
class ZipError extends Error {
  readonly reason: "bomb" | "invalid" | "link"

  constructor(reason: "bomb" | "invalid" | "link") {
    super(reason)
    this.reason = reason
  }
}

const JUNK = /(^|\/)(__MACOSX\/|\.DS_Store$)/

const UNIX = 3
const FILE_TYPE = 0o170000
const SYMLINK = 0o120000

interface Attributes {
  /** Unix permission bits and file type, when the zip was made on Unix. */
  mode: number
}

function endOfDirectory(view: DataView): number {
  let end = view.byteLength - 22
  while (end >= 0 && view.getUint32(end, true) !== 0x06054b50) end--
  return end
}

/** The Unix modes in the zip's central directory, by entry name. Without them: empty. */
function attributesOf(zip: Uint8Array): Map<string, Attributes> {
  const out = new Map<string, Attributes>()
  const view = new DataView(zip.buffer, zip.byteOffset, zip.byteLength)
  const end = endOfDirectory(view)
  if (end < 0) return out
  let at = view.getUint32(end + 16, true)
  for (let n = view.getUint16(end + 10, true); n > 0; n--) {
    if (at + 46 > zip.length || view.getUint32(at, true) !== 0x02014b50) break
    const nameLength = view.getUint16(at + 28, true)
    const next =
      at + 46 + nameLength + view.getUint16(at + 30, true) + view.getUint16(at + 32, true)
    if (next > zip.length) break
    if (view.getUint8(at + 5) === UNIX) {
      const utf8 = (view.getUint16(at + 8, true) & 0x800) !== 0
      const name = new TextDecoder(utf8 ? "utf-8" : "latin1").decode(
        zip.subarray(at + 46, at + 46 + nameLength),
      )
      out.set(name, { mode: view.getUint32(at + 38, true) >>> 16 })
    }
    at = next
  }
  return out
}

/** Whether a link at `entry` with `target` stays inside the folder, as Zeabur Builder checks. */
export function linkStaysInside(entry: string, target: string): boolean {
  let depth = entry.split("/").length - 1
  let named = false
  if (target === "" || target.startsWith("/")) return false
  for (const part of target.split("/")) {
    if (part === "" || part === ".") continue
    if (part !== "..") named = true
    else if (named || depth-- <= 0) return false
  }
  return true
}

/** The files of a zip, without the one folder most zips wrap them in. */
export async function filesOfZip(zip: Blob): Promise<SourceFile[]> {
  let total = 0
  let bomb = false
  let found: Record<string, Uint8Array>
  const bytes = new Uint8Array(await zip.arrayBuffer())
  try {
    found = unzipSync(bytes, {
      filter: (entry) => {
        total += entry.originalSize
        if (total > MAX_UNPACKED) bomb = true
        return !bomb && !entry.name.endsWith("/") && !JUNK.test(entry.name)
      },
    })
  } catch {
    throw new ZipError(bomb ? "bomb" : "invalid")
  }
  if (bomb) throw new ZipError("bomb")
  const attributes = attributesOf(bytes)
  const files = withoutRoot(
    Object.entries(found)
      .filter(([name]) => !name.startsWith("/") && !name.split("/").includes(".."))
      .map(([name, data]): SourceFile => {
        const mode = attributes.get(name)?.mode
        const path = name
          .split("/")
          .filter((part) => part !== "" && part !== ".")
          .join("/")
        if (mode !== undefined && (mode & FILE_TYPE) === SYMLINK)
          return { path, data: new Blob([]), link: new TextDecoder().decode(data) }
        return {
          path,
          data: new Blob([data as Uint8Array<ArrayBuffer>]),
          ...(mode === undefined || mode === 0 ? {} : { executable: (mode & 0o111) !== 0 }),
        }
      }),
  )
  const links = new Set(files.filter((f) => f.link !== undefined).map((f) => f.path))
  for (const file of files) {
    if (file.link !== undefined && !linkStaysInside(file.path, file.link))
      throw new ZipError("link")
    const parts = file.path.split("/")
    for (let i = 1; i < parts.length; i++)
      if (links.has(parts.slice(0, i).join("/"))) throw new ZipError("link")
  }
  return files
}
