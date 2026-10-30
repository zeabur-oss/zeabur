// The Variables tab's editor: the variables set directly on a service's containers as rows to
// edit, add and remove, and the one env change per container that carries them.
import type { Schemas } from "@/api/client"

export interface EnvRow {
  id: string
  container: string
  name: string
  value: string
  /** As in the cluster; absent for an added row. */
  was?: { name: string; value: string }
  removed: boolean
}

export type RowState = "same" | "changed" | "added" | "removed"

export type RowProblem = "empty" | "invalid" | "duplicate" | "taken"

/** Kubernetes also accepts dots and dashes, which a shell cannot read. */
const nameRule = /^[-._a-zA-Z][-._a-zA-Z0-9]*$/

export function rowState(row: EnvRow): RowState {
  if (!row.was) return "added"
  if (row.removed) return "removed"
  return row.name === row.was.name && row.value === row.was.value ? "same" : "changed"
}

/** An added row nothing has been typed in. */
const blank = (row: EnvRow) => !row.was && row.name.trim() === "" && row.value === ""

const live = (row: EnvRow) => !row.removed && !blank(row)

export const editedCount = (rows: readonly EnvRow[]) =>
  rows.filter((row) => !blank(row) && rowState(row) !== "same").length

/** What is wrong with each row's name; `taken` holds, by container, the names another source
 * (a Secret, a config file, Kubernetes) already sets. */
export function problemsOf(
  rows: readonly EnvRow[],
  taken: ReadonlyMap<string, ReadonlySet<string>>,
): Map<string, RowProblem> {
  const counts = new Map<string, number>()
  for (const row of rows.filter(live)) {
    const key = `${row.container}/${row.name.trim()}`
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  const problems = new Map<string, RowProblem>()
  for (const row of rows.filter(live)) {
    const name = row.name.trim()
    if (name === "") problems.set(row.id, "empty")
    else if (!nameRule.test(name)) problems.set(row.id, "invalid")
    else if ((counts.get(`${row.container}/${name}`) ?? 0) > 1) problems.set(row.id, "duplicate")
    else if (taken.get(row.container)?.has(name)) problems.set(row.id, "taken")
  }
  return problems
}

/** The edits of each container that has any: removals first, then what is set. */
export function changesOf(
  rows: readonly EnvRow[],
): { container: string; changes: Schemas.EnvEdit[] }[] {
  const containers = [...new Set(rows.map((row) => row.container))]
  return containers.flatMap((container) => {
    const own = rows.filter((row) => row.container === container)
    const sets: Schemas.EnvEdit[] = own
      .filter((row) => live(row) && rowState(row) !== "same")
      .map((row) => ({ name: row.name.trim(), op: "set", value: row.value }))
    const setNames = new Set(sets.map((edit) => edit.name))
    const removes = own.flatMap((row): Schemas.EnvEdit[] =>
      row.was && (row.removed || row.name.trim() !== row.was.name) && !setNames.has(row.was.name)
        ? [{ name: row.was.name, op: "remove" }]
        : [],
    )
    const changes = [...removes, ...sets]
    return changes.length > 0 ? [{ container, changes }] : []
  })
}

export interface Parsed {
  entries: { name: string; value: string; line: number }[]
  /** Lines that are not `NAME=value`, a comment or blank, counted from 1. */
  unread: number[]
}

const escapes: Record<string, string> = { n: "\n", r: "\r", t: "\t", '"': '"', "\\": "\\" }

/** `.env` text: `NAME=value` lines, `export ` before a name, `#` comments, blank lines, values in
 * single quotes (as written) or double quotes (with `\n`, `\"`, `\\`, `\uXXXX`; may span lines). */
export function parseDotenv(text: string): Parsed {
  const entries: Parsed["entries"] = []
  const unread: number[] = []
  const lines = text.replace(/\r\n?/g, "\n").split("\n")
  for (let i = 0; i < lines.length; i++) {
    const line = i + 1
    const trimmed = (lines[i] ?? "").trim()
    if (trimmed === "" || trimmed.startsWith("#")) continue
    const statement = trimmed.replace(/^export\s+/, "")
    const equals = statement.indexOf("=")
    if (equals <= 0) {
      unread.push(line)
      continue
    }
    const name = statement.slice(0, equals).trim()
    let rest = statement.slice(equals + 1).trimStart()
    let value: string
    if (rest.startsWith('"')) {
      let out = ""
      let at = 1
      let closed = false
      for (;;) {
        while (at < rest.length) {
          const c = rest[at] as string
          if (c === '"') {
            closed = true
            break
          }
          if (c === "\\" && at + 1 < rest.length) {
            const next = rest[at + 1] as string
            if (next === "u" && /^[0-9a-fA-F]{4}$/.test(rest.slice(at + 2, at + 6))) {
              out += String.fromCharCode(parseInt(rest.slice(at + 2, at + 6), 16))
              at += 6
              continue
            }
            out += escapes[next] ?? `\\${next}`
            at += 2
            continue
          }
          out += c
          at++
        }
        if (closed || i + 1 >= lines.length) break
        i++
        out += "\n"
        rest = lines[i] ?? ""
        at = 0
      }
      if (!closed) {
        unread.push(line)
        continue
      }
      value = out
    } else if (rest.startsWith("'")) {
      const end = rest.indexOf("'", 1)
      if (end === -1) {
        unread.push(line)
        continue
      }
      value = rest.slice(1, end)
    } else {
      value = rest.replace(/\s+#.*$/, "").trimEnd()
    }
    entries.push({ name, value, line })
  }
  return { entries, unread }
}

/** A value as one `.env` line can hold it. */
export function quote(value: string): string {
  return /[\s#"'\\]/.test(value) || value === "" ? JSON.stringify(value) : value
}

export const toDotenv = (rows: readonly EnvRow[]) =>
  rows
    .filter(live)
    .map((row) => `${row.name.trim()}=${quote(row.value)}`)
    .join("\n")

/** The rows `text` makes of one container's: a name the cluster has keeps its row, one it lacks
 * is added, and a row the text leaves out is removed. */
export function fromDotenv(
  rows: readonly EnvRow[],
  container: string,
  entries: Parsed["entries"],
): EnvRow[] {
  const own = rows.filter((row) => row.container === container && row.was)
  const used = new Set<string>()
  const kept = new Map<string, EnvRow>()
  const added: EnvRow[] = []
  for (const entry of entries) {
    const row = own.find((r) => r.was?.name === entry.name && !used.has(r.id))
    if (row) {
      used.add(row.id)
      kept.set(row.id, { ...row, name: entry.name, value: entry.value, removed: false })
    } else
      added.push({
        id: `text-${entry.line}`,
        container,
        name: entry.name,
        value: entry.value,
        removed: false,
      })
  }
  return [
    ...rows.filter((row) => row.container !== container),
    ...own.map((row) => kept.get(row.id) ?? { ...row, removed: true }),
    ...added,
  ]
}

/** Pasted `NAME=value` lines put into `rows` from the row pasted in: the first fills that row, a
 * name another row has sets that row's value, and the rest are added after it. */
export function pasteInto(
  rows: readonly EnvRow[],
  at: string,
  entries: Parsed["entries"],
  newId: () => string,
): EnvRow[] {
  const target = rows.find((row) => row.id === at)
  if (!target) return [...rows]
  const result = [...rows]
  const [first, ...more] = entries
  if (!first) return result
  const index = result.findIndex((row) => row.id === at)
  result[index] = { ...target, name: first.name, value: first.value, removed: false }
  const extra: EnvRow[] = []
  for (const entry of more) {
    const same = result.findIndex(
      (row) => row.container === target.container && !row.removed && row.name.trim() === entry.name,
    )
    if (same !== -1) result[same] = { ...(result[same] as EnvRow), value: entry.value }
    else
      extra.push({
        id: newId(),
        container: target.container,
        name: entry.name,
        value: entry.value,
        removed: false,
      })
  }
  result.splice(index + 1, 0, ...extra)
  return result
}

/** Whether pasted text is `NAME=value` lines rather than a name. */
export function pastedLines(text: string): Parsed["entries"] | undefined {
  if (!text.includes("=") && !text.includes("\n")) return undefined
  const { entries } = parseDotenv(text)
  return entries.length > 0 ? entries : undefined
}
