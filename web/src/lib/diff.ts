// A line diff of two texts, for a plan's before and after and for two releases, which the server
// writes as YAML. The diff stays here: the Releases tab compares any two releases the person
// picks, so the server would have to send every pair.

export type DiffLine = { kind: "same" | "add" | "remove"; text: string }

/** A line diff (longest common subsequence), unchanged lines included. */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = before ? before.split("\n") : []
  const b = after ? after.split("\n") : []
  const n = a.length
  const m = b.length
  // lengths[i][j]: the longest common subsequence of a[i..] and b[j..].
  const lengths = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      lengths[i]![j] =
        a[i] === b[j]
          ? lengths[i + 1]![j + 1]! + 1
          : Math.max(lengths[i + 1]![j]!, lengths[i]![j + 1]!)
  const out: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ kind: "same", text: a[i]! })
      i++
      j++
    } else if (lengths[i + 1]![j]! >= lengths[i]![j + 1]!)
      out.push({ kind: "remove", text: a[i++]! })
    else out.push({ kind: "add", text: b[j++]! })
  }
  while (i < n) out.push({ kind: "remove", text: a[i++]! })
  while (j < m) out.push({ kind: "add", text: b[j++]! })
  return out
}

/** The changed lines with `context` unchanged lines around each change; `null` marks a gap. */
export function hunks(diff: DiffLine[], context = 3): (DiffLine | null)[] {
  const keep = new Uint8Array(diff.length)
  diff.forEach((line, index) => {
    if (line.kind === "same") return
    for (let k = Math.max(0, index - context); k <= Math.min(diff.length - 1, index + context); k++)
      keep[k] = 1
  })
  const out: (DiffLine | null)[] = []
  let gap = false
  diff.forEach((line, index) => {
    if (keep[index]) {
      if (gap && out.length > 0) out.push(null)
      gap = false
      out.push(line)
    } else gap = true
  })
  return out
}
