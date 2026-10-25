// The files of a zip. Apart from source.ts so that the unzip code loads only for a zip.
import { unzipSync } from "fflate"

import { MAX_UNPACKED } from "@/lib/pack/limits"
import { withoutRoot, type SourceFile } from "@/lib/pack/source"

/** The zip would unpack to more than the limit, or is not a zip. */
class ZipError extends Error {
  readonly reason: "bomb" | "invalid"

  constructor(reason: "bomb" | "invalid") {
    super(reason)
    this.reason = reason
  }
}

const JUNK = /(^|\/)(__MACOSX\/|\.DS_Store$)/

/** The files of a zip, without the one folder most zips wrap them in. */
export async function filesOfZip(zip: Blob): Promise<SourceFile[]> {
  let total = 0
  let bomb = false
  let found: Record<string, Uint8Array>
  try {
    found = unzipSync(new Uint8Array(await zip.arrayBuffer()), {
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
  return withoutRoot(
    Object.entries(found)
      .filter(([name]) => !name.startsWith("/") && !name.split("/").includes(".."))
      .map(([path, bytes]) => ({ path, data: new Blob([bytes as Uint8Array<ArrayBuffer>]) })),
  )
}
