// Splits a log line into text segments styled by its ANSI SGR escape sequences (ECMA-48 §8.3.117).
// The result is text and class names only: never HTML, so a log cannot inject markup. Colours map
// to the 16 standard colours; 256-colour and true-colour arguments are consumed and ignored. Every
// other escape sequence is dropped.

export interface Segment {
  /** Where the segment starts in the line's text without escapes (a stable key). */
  start: number
  text: string
  className: string
}

interface Style {
  fg?: number
  bg?: number
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
}

// Class names written out in full, so Tailwind finds them in this file.
const foreground: Record<number, string> = {
  30: "text-neutral-900 dark:text-neutral-400",
  31: "text-red-700 dark:text-red-400",
  32: "text-green-700 dark:text-green-400",
  33: "text-yellow-700 dark:text-yellow-300",
  34: "text-blue-700 dark:text-blue-400",
  35: "text-fuchsia-700 dark:text-fuchsia-400",
  36: "text-cyan-700 dark:text-cyan-400",
  37: "text-neutral-500 dark:text-neutral-200",
  90: "text-neutral-500 dark:text-neutral-500",
  91: "text-red-600 dark:text-red-300",
  92: "text-green-600 dark:text-green-300",
  93: "text-yellow-600 dark:text-yellow-200",
  94: "text-blue-600 dark:text-blue-300",
  95: "text-fuchsia-600 dark:text-fuchsia-300",
  96: "text-cyan-600 dark:text-cyan-300",
  97: "text-neutral-400 dark:text-white",
}

const background: Record<number, string> = {
  40: "bg-neutral-900/15 dark:bg-neutral-950",
  41: "bg-red-500/20",
  42: "bg-green-500/20",
  43: "bg-yellow-500/20",
  44: "bg-blue-500/20",
  45: "bg-fuchsia-500/20",
  46: "bg-cyan-500/20",
  47: "bg-neutral-500/20",
  100: "bg-neutral-500/30",
  101: "bg-red-400/25",
  102: "bg-green-400/25",
  103: "bg-yellow-400/25",
  104: "bg-blue-400/25",
  105: "bg-fuchsia-400/25",
  106: "bg-cyan-400/25",
  107: "bg-neutral-300/30",
}

function classes(style: Style): string {
  const names: string[] = []
  if (style.fg !== undefined) names.push(foreground[style.fg] ?? "")
  if (style.bg !== undefined) names.push(background[style.bg] ?? "")
  if (style.bold) names.push("font-bold")
  if (style.dim) names.push("opacity-70")
  if (style.italic) names.push("italic")
  if (style.underline) names.push("underline")
  return names.filter(Boolean).join(" ")
}

function apply(style: Style, params: number[]): Style {
  const next = { ...style }
  for (let i = 0; i < params.length; i++) {
    const p = params[i] ?? 0
    if (p === 0) {
      for (const key of Object.keys(next)) delete next[key as keyof Style]
    } else if (p === 1) next.bold = true
    else if (p === 2) next.dim = true
    else if (p === 3) next.italic = true
    else if (p === 4) next.underline = true
    else if (p === 22) next.bold = next.dim = false
    else if (p === 23) next.italic = false
    else if (p === 24) next.underline = false
    else if ((p >= 30 && p <= 37) || (p >= 90 && p <= 97)) next.fg = p
    else if (p === 39) delete next.fg
    else if ((p >= 40 && p <= 47) || (p >= 100 && p <= 107)) next.bg = p
    else if (p === 49) delete next.bg
    else if (p === 38 || p === 48) {
      // 38;5;n or 38;2;r;g;b (and 48 for the background): skip the arguments.
      const mode = params[i + 1]
      i += mode === 5 ? 2 : mode === 2 ? 4 : 1
    }
  }
  return next
}

// CSI sequences (ESC [ params final-byte); OSC strings (ESC ] … BEL or ST), such as a window
// title or a hyperlink; and every other escape sequence, which ECMA-35 shapes as ESC, any
// intermediate bytes (space to `/`) and one final byte (`0` to `~`), such as the ESC ( B that
// `tput sgr0` writes.
const escape =
  // oxlint-disable-next-line no-control-regex -- matching escape sequences is the point
  /\u001b\[([0-?]*)[ -/]*([@-~])|\u001b\][^\u0007\u001b]*(\u0007|\u001b\\)?|\u001b[ -/]*[0-~]/g

export function parseAnsi(line: string): Segment[] {
  const segments: Segment[] = []
  let style: Style = {}
  let last = 0
  let length = 0
  const push = (text: string) => {
    if (!text) return
    const className = classes(style)
    const previous = segments.at(-1)
    if (previous && previous.className === className) previous.text += text
    else segments.push({ start: length, text, className })
    length += text.length
  }
  for (const match of line.matchAll(escape)) {
    push(line.slice(last, match.index))
    last = match.index + match[0].length
    if (match[2] === "m") {
      const params = (match[1] ?? "").split(";").map((p) => (p === "" ? 0 : Number(p)))
      style = apply(style, params)
    }
  }
  push(line.slice(last))
  return segments
}
