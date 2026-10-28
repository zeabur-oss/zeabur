// Prepares dist/ for embedding in the Rust binary (crates/zeabur-ui/build.rs):
//   - fails if index.html carries an inline script or style (the CSP allows neither);
//   - writes a brotli (.br) encoding of every compressible file, when smaller;
//   - writes manifest.tsv: path, MIME type, SHA-256, cache class, encodings.
// Output is deterministic: the same dist/ gives the same bytes.
import { createHash } from "node:crypto"
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs"
import { join, relative } from "node:path"
import { brotliCompressSync, constants } from "node:zlib"

const dist = process.argv[2] ?? "dist"

const types: Record<string, [string, boolean]> = {
  html: ["text/html; charset=utf-8", true],
  js: ["text/javascript; charset=utf-8", true],
  css: ["text/css; charset=utf-8", true],
  json: ["application/json", true],
  webmanifest: ["application/manifest+json", true],
  svg: ["image/svg+xml", true],
  txt: ["text/plain; charset=utf-8", true],
  woff2: ["font/woff2", false],
  png: ["image/png", false],
  ico: ["image/x-icon", false],
}

function files(dir: string): string[] {
  return readdirSync(dir)
    .toSorted()
    .flatMap((name) => {
      const path = join(dir, name)
      if (statSync(path).isDirectory()) return files(path)
      if (name.endsWith(".br") || name === "manifest.tsv") return []
      return [path]
    })
}

const index = readFileSync(join(dist, "index.html"), "utf8")
for (const tag of index.match(/<script\b[^>]*>/gi) ?? []) {
  if (!/\bsrc=/i.test(tag)) throw new Error(`index.html has an inline script: ${tag}`)
}
if (/<style\b|\sstyle=/i.test(index)) throw new Error("index.html has an inline style")

const lines: string[] = []
for (const file of files(dist)) {
  const path = relative(dist, file).split("\\").join("/")
  const extension = path.slice(path.lastIndexOf(".") + 1)
  const type = types[extension]
  if (!type) throw new Error(`${path}: no MIME type for .${extension}`)
  const [mime, compressible] = type
  const bytes = readFileSync(file)
  const sha256 = createHash("sha256").update(bytes).digest("hex")
  const cache = path.startsWith("assets/") ? "immutable" : "revalidate"
  const encodings: string[] = []
  if (compressible) {
    const br = brotliCompressSync(bytes, {
      params: {
        [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
        [constants.BROTLI_PARAM_SIZE_HINT]: bytes.length,
      },
    })
    if (br.length < bytes.length) {
      writeFileSync(`${file}.br`, br)
      encodings.push("br")
    }
  }
  lines.push([path, mime, sha256, cache, encodings.join(",") || "-"].join("\t"))
}
writeFileSync(join(dist, "manifest.tsv"), lines.join("\n") + "\n")
process.stdout.write(`manifest.tsv: ${lines.length} files\n`)
