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

/** The archive with its file count and unpacked size; it is sent unchanged. */
export async function inspectTarGz(archive: Blob): Promise<Packed> {
  if (archive.size > MAX_PACKED) throw new TarGzError("packed")
  let files = 0
  let bytes = 0
  let seen = 0
  let skip = 0
  let carry: Uint8Array = new Uint8Array(0)
  let ended = false
  try {
    const reader = archive.stream().pipeThrough(new DecompressionStream("gzip")).getReader()
    while (!ended) {
      const { done, value } = await reader.read()
      if (done) break
      let chunk: Uint8Array = value
      seen += chunk.length
      while (chunk.length > 0 && !ended) {
        if (skip > 0) {
          const n = Math.min(skip, chunk.length)
          skip -= n
          chunk = chunk.subarray(n)
          continue
        }
        const need = BLOCK - carry.length
        if (chunk.length < need) {
          carry = join(carry, chunk)
          break
        }
        const block = join(carry, chunk.subarray(0, need))
        carry = new Uint8Array(0)
        chunk = chunk.subarray(need)
        if (block.every((byte) => byte === 0)) {
          ended = true
          break
        }
        checked(block)
        const size = sizeOf(block)
        const type = field(block, 156, 157)
        if (type === "" || type === "0" || type === "7") {
          files++
          bytes += size
          if (bytes > MAX_UNPACKED) throw new TarGzError("bomb")
        }
        skip = Math.ceil(size / BLOCK) * BLOCK
      }
    }
    await reader.cancel()
  } catch (error) {
    throw error instanceof TarGzError ? error : new TarGzError("invalid")
  }
  if (seen === 0) throw new TarGzError("invalid")
  return { blob: archive, files, left: 0, bytes }
}
