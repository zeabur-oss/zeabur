// A tar writer for Zeabur Builder's archive source: regular files and folders, with a PAX record
// for a path that does not fit the 100 bytes of a header (POSIX.1-2001, "pax Interchange Format").
const BLOCK = 512
const encoder = new TextEncoder()

export interface TarFile {
  path: string
  data: Blob
  executable: boolean
}

function octal(value: number, width: number): string {
  return value.toString(8).padStart(width - 1, "0") + "\0"
}

type Bytes = Uint8Array<ArrayBuffer>

function header(name: string, size: number, mode: number, type: "0" | "5" | "x"): Bytes {
  const block = new Uint8Array(BLOCK)
  const put = (text: string, at: number) => block.set(encoder.encode(text), at)
  put(name, 0)
  put(octal(mode, 8), 100)
  put(octal(0, 8), 108)
  put(octal(0, 8), 116)
  put(octal(size, 12), 124)
  put(octal(0, 12), 136)
  put("        ", 148)
  put(type, 156)
  put("ustar\0" + "00", 257)
  const sum = block.reduce((total, byte) => total + byte, 0)
  put(octal(sum, 7) + " ", 148)
  return block
}

const padding = (size: number): Bytes => new Uint8Array((BLOCK - (size % BLOCK)) % BLOCK)

/** `<length> path=<path>\n`, where the length counts its own digits. */
function paxRecord(path: string): Bytes {
  const body = ` path=${path}\n`
  const bytes = encoder.encode(body).length
  let length = bytes + String(bytes).length
  if (String(length).length !== String(bytes).length) length = bytes + String(length).length
  return encoder.encode(`${length}${body}`)
}

function entry(path: string, size: number, mode: number, type: "0" | "5"): Bytes[] {
  const name = type === "5" ? `${path}/` : path
  if (encoder.encode(name).length <= 100) return [header(name, size, mode, type)]
  const record = paxRecord(path)
  return [
    header("PaxHeader/entry", record.length, 0o644, "x"),
    record,
    padding(record.length),
    header("long-name", size, mode, type),
  ]
}

/** The tar of `files`, in path order, with the folders they sit in. */
export function tarParts(files: readonly TarFile[]): BlobPart[] {
  const parts: BlobPart[] = []
  const folders = new Set<string>()
  const sorted = [...files].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
  for (const file of sorted) {
    const segments = file.path.split("/")
    for (let i = 1; i < segments.length; i++) {
      const folder = segments.slice(0, i).join("/")
      if (folders.has(folder)) continue
      folders.add(folder)
      parts.push(...entry(folder, 0, 0o755, "5"))
    }
    parts.push(...entry(file.path, file.data.size, file.executable ? 0o755 : 0o644, "0"))
    parts.push(file.data, padding(file.data.size))
  }
  parts.push(new Uint8Array(BLOCK * 2))
  return parts
}
