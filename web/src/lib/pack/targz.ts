// Checks a .tar.gz a person gives and sends it as it is, so modes and symlinks stay.
// Apart from source.ts so that the code loads only for an archive.
import { MAX_PACKED, MAX_UNPACKED } from "@/lib/pack/limits"
import type { Packed } from "@/lib/pack/pack"

const BLOCK = 512

/** Not a gzip tar, past the unpacked limit, or past the packed limit. */
class TarGzError extends Error {
  readonly reason: "bomb" | "invalid" | "packed"

  constructor(reason: "bomb" | "invalid" | "packed") {
    super(reason)
    this.reason = reason
  }
}

const decoder = new TextDecoder()

function field(block: Uint8Array, from: number, to: number): string {
  return decoder.decode(block.subarray(from, to)).replace(/\0.*$/, "").trim()
}

function sizeOf(block: Uint8Array): number {
  // A size over 8 GiB is stored as a big-endian number after a 0x80 flag.
  if ((block[124] ?? 0) & 0x80) {
    let size = (block[124] ?? 0) & 0x7f
    for (let i = 125; i < 136; i++) size = size * 256 + (block[i] ?? 0)
    return size
  }
  const size = parseInt(field(block, 124, 136) || "0", 8)
  if (Number.isNaN(size)) throw new TarGzError("invalid")
  return size
}

function checked(block: Uint8Array) {
  let sum = 0
  for (let i = 0; i < BLOCK; i++) sum += i >= 148 && i < 156 ? 32 : (block[i] ?? 0)
  if (sum !== parseInt(field(block, 148, 156) || "-1", 8)) throw new TarGzError("invalid")
}

function join(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length)
  out.set(a)
  out.set(b, a.length)
  return out
}

/** Walks the tar headers of the unpacked bytes, which come in chunks. */
class Scanner {
  files = 0
  bytes = 0
  seen = 0
  ended = false
  private skip = 0
  private carry: Uint8Array = new Uint8Array(0)

  push(chunk: Uint8Array) {
    this.seen += chunk.length
    let rest = chunk
    while (rest.length > 0 && !this.ended) {
      if (this.skip > 0) {
        const n = Math.min(this.skip, rest.length)
        this.skip -= n
        rest = rest.subarray(n)
        continue
      }
      const need = BLOCK - this.carry.length
      if (rest.length < need) {
        this.carry = join(this.carry, rest)
        return
      }
      const block = join(this.carry, rest.subarray(0, need))
      this.carry = new Uint8Array(0)
      rest = rest.subarray(need)
      this.header(block)
    }
  }

  private header(block: Uint8Array) {
    if (block.every((byte) => byte === 0)) {
      this.ended = true
      return
    }
    checked(block)
    const size = sizeOf(block)
    const type = field(block, 156, 157)
    if (type === "" || type === "0" || type === "7") {
      this.files++
      this.bytes += size
      if (this.bytes > MAX_UNPACKED) throw new TarGzError("bomb")
    }
    this.skip = Math.ceil(size / BLOCK) * BLOCK
  }
}

/** The archive with its file count and unpacked size; it is sent unchanged. */
export async function inspectTarGz(archive: Blob): Promise<Packed> {
  if (archive.size > MAX_PACKED) throw new TarGzError("packed")
  const scan = new Scanner()
  try {
    const reader = archive.stream().pipeThrough(new DecompressionStream("gzip")).getReader()
    for (let next = await reader.read(); !next.done && !scan.ended; next = await reader.read())
      scan.push(next.value)
    await reader.cancel()
  } catch (error) {
    throw error instanceof TarGzError ? error : new TarGzError("invalid")
  }
  if (scan.seen === 0) throw new TarGzError("invalid")
  return { blob: archive, files: scan.files, left: 0, bytes: scan.bytes }
}
