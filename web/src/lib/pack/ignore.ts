// Which files of a folder go up: `.gitignore` files at any depth, as Git reads them, and a
// `.dockerignore` at the top, with every pattern starting there, as Docker reads it. A path is
// left out when either says so, or when a folder above it is. Never `.git`.
import ignore, { type Ignore } from "ignore"

export interface IgnoreFile {
  /** The folder it sits in, relative to the top; empty for the top. */
  folder: string
  kind: "git" | "docker"
  text: string
}

const dirname = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf("/")))

/** Docker's patterns start at the top; gitignore anchors with a leading slash. */
function anchored(text: string): string {
  return text
    .split(/\r?\n/)
    .map((line) => {
      const negated = line.startsWith("!")
      const body = negated ? line.slice(1) : line
      if (body.trim() === "" || body.startsWith("#")) return line
      return `${negated ? "!" : ""}/${body.replace(/^\/+/, "")}`
    })
    .join("\n")
}

export function excluder(files: readonly IgnoreFile[]) {
  const git = new Map<string, Ignore>()
  let docker: Ignore | undefined
  for (const file of files) {
    if (file.kind === "git") git.set(file.folder, ignore().add(file.text))
    else if (file.folder === "") docker = ignore().add(anchored(file.text))
  }
  const folders = new Map<string, boolean>()

  /** The folders from the top down to the one that holds `path`. */
  const chainOf = (path: string): string[] => {
    const parent = dirname(path)
    const parts = parent === "" ? [] : parent.split("/")
    return ["", ...parts.map((_, i) => parts.slice(0, i + 1).join("/"))]
  }

  /** Whether the rules of the folders above `path` leave it out; the deepest word wins. */
  const decided = (path: string, directory: boolean): boolean => {
    const asked = directory ? `${path}/` : path
    let out = false
    for (const folder of chainOf(path)) {
      const result = git.get(folder)?.test(folder === "" ? asked : asked.slice(folder.length + 1))
      if (result?.ignored) out = true
      else if (result?.unignored) out = false
    }
    return out || !!docker?.ignores(asked)
  }

  const folderOut = (folder: string): boolean => {
    if (folder === "") return false
    const known = folders.get(folder)
    if (known !== undefined) return known
    const out =
      folder.split("/").pop() === ".git" || folderOut(dirname(folder)) || decided(folder, true)
    folders.set(folder, out)
    return out
  }

  return {
    /** Whether the file at `path` is left out. */
    file: (path: string) => folderOut(dirname(path)) || decided(path, false),
  }
}

export const isIgnoreFile = (path: string): "git" | "docker" | undefined => {
  const name = path.split("/").pop()
  return name === ".gitignore" ? "git" : name === ".dockerignore" ? "docker" : undefined
}
