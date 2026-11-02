// What a Dockerfile exposes, read the way the Dockerfile reference parses it:
// https://docs.docker.com/reference/dockerfile/
import { portProblem } from "@/model/image-service"

type Vars = Map<string, string | undefined>
type Lookup = (name: string) => string | undefined

interface Stage {
  name: string
  args: Vars
  env: Vars
  ports: string[]
}

const UNKNOWN = "\0"
const VARIABLE = String.raw`\$(?:\{([A-Za-z_]\w*)(?:(:?)([-+])([^}]*))?\}|([A-Za-z_]\w*))`
const HEREDOC = /(?<!<)<<(-?)(["']?)([\w.-]+)\2/g

/** The first TCP port a Dockerfile's last stage `EXPOSE`s, with `ARG` and `ENV` values filled in;
 * a port that cannot be worked out is skipped, and none is an empty string. */
export function exposedPort(text = ""): string {
  const lines = text.split(/\r?\n/)
  const escape = escapeOf(lines)
  const globals: Vars = new Map()
  const stages: Stage[] = []
  for (const instruction of instructions(lines, escape)) {
    const [, word = "", rest = ""] = /^\s*(\S+)\s*(.*)$/s.exec(instruction) ?? []
    const keyword = word.toUpperCase()
    const stage = stages.at(-1)
    if (keyword === "FROM") stages.push(from(rest, globals, stages, escape))
    else if (stage !== undefined) apply(stage, keyword, rest, globals, escape)
    else if (keyword === "ARG")
      for (const arg of expand(rest, (n) => globals.get(n), escape)) assign(globals, arg, globals)
  }
  return stages.at(-1)?.ports[0] ?? ""
}

function escapeOf(lines: string[]): string {
  for (const line of lines) {
    const directive = /^\s*#\s*(\w+)\s*=\s*(\S+)\s*$/.exec(line)
    if (directive === null) break
    if (directive[1]?.toLowerCase() === "escape") return directive[2] === "`" ? "`" : "\\"
  }
  return "\\"
}

const escaping = (escape: string) => (escape === "`" ? "`" : "\\\\")

/** The instructions with their continuation lines joined, leaving out comments, empty lines and
 * the bodies of heredocs. */
function instructions(lines: string[], escape: string): string[] {
  const continued = new RegExp(`${escaping(escape)}[ \\t]*$`)
  const out: string[] = []
  let pending = ""
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? ""
    if (/^\s*(#|$)/.test(line)) continue
    if (continued.test(line)) {
      pending += line.replace(continued, "")
      continue
    }
    out.push(pending + line)
    i = afterHeredocs(lines, i, pending + line)
    pending = ""
  }
  return out
}

function afterHeredocs(lines: string[], at: number, instruction: string): number {
  if (!/^\s*(RUN|COPY|ADD)\s/i.test(instruction)) return at
  for (const [, dash, , word] of instruction.matchAll(HEREDOC)) {
    const end = lines.findIndex(
      (line, i) => i > at && (dash === "-" ? line.replace(/^\t+/, "") : line) === word,
    )
    at = end < 0 ? lines.length : end
  }
  return at
}

function from(rest: string, globals: Vars, stages: Stage[], escape: string): Stage {
  const [base = "", as, name = ""] = expand(rest, (n) => globals.get(n), escape).filter(
    (word) => !word.startsWith("--"),
  )
  const parent = stages.find((stage) => stage.name === base.toLowerCase())
  return {
    name: as?.toLowerCase() === "as" ? name.toLowerCase() : "",
    args: new Map(parent?.args),
    env: new Map(parent?.env),
    ports: [...(parent?.ports ?? [])],
  }
}

function apply(stage: Stage, keyword: string, rest: string, globals: Vars, escape: string) {
  const lookup: Lookup = (n) => (stage.env.has(n) ? stage.env.get(n) : stage.args.get(n))
  if (keyword === "ARG")
    for (const arg of expand(rest, lookup, escape)) assign(stage.args, arg, globals)
  if (keyword === "ENV") for (const pair of envPairs(rest, lookup, escape)) assign(stage.env, pair)
  if (keyword === "EXPOSE") stage.ports.push(...expand(rest, lookup, escape).flatMap(tcpPorts))
}

/** `name=value` sets the name; a bare `ARG name` takes the value a global `ARG` gave it. */
function assign(vars: Vars, word: string, globals?: Vars) {
  const at = word.indexOf("=")
  if (at > 0) vars.set(word.slice(0, at), word.slice(at + 1))
  else if (at < 0 && globals !== undefined) vars.set(word, globals.get(word))
}

/** `ENV name=value ...`, or the older `ENV name value` that sets one name to the rest. */
function envPairs(rest: string, lookup: Lookup, escape: string): string[] {
  const legacy = /^([^\s=]+)\s+(.*)$/s.exec(rest)
  if (legacy === null) return expand(rest, lookup, escape)
  return [`${legacy[1]}=${expand(legacy[2] ?? "", lookup, escape).join(" ")}`]
}

const tcpPorts = (word: string) =>
  word.split(/\s+/).flatMap((token) => {
    const [, first = "", protocol = "tcp"] = /^(\d{1,5})(?:-\d+)?(?:\/(\w+))?$/.exec(token) ?? []
    return protocol.toLowerCase() === "tcp" && first !== "" && !portProblem(first)
      ? [String(Number(first))]
      : []
  })

/** The words of `text` with quotes taken off, escapes applied and variables filled in; a variable
 * that is not set leaves `UNKNOWN` in its word. */
function expand(text: string, lookup: Lookup, escape: string): string[] {
  const e = escaping(escape)
  const words = text.match(
    new RegExp(`(?:'[^']*'?|"(?:${e}[\\s\\S]|[^"${e}])*"?|${e}[\\s\\S]?|[^\\s'"${e}])+`, "g"),
  )
  const part = new RegExp(
    `'([^']*)'?|"((?:${e}[\\s\\S]|[^"${e}])*)"?|((?:${e}[\\s\\S]?|[^'"${e}])+)`,
    "g",
  )
  return (words ?? []).map((word) =>
    word.replace(
      part,
      (_, single?: string, double?: string, plain?: string) =>
        single ?? substitute(double ?? plain ?? "", lookup, escape, double !== undefined),
    ),
  )
}

/** Escapes and variables outside single quotes; inside double quotes the escape character keeps
 * its meaning only before `"`, `$` and itself. */
function substitute(text: string, lookup: Lookup, escape: string, quoted: boolean): string {
  const pattern = new RegExp(`${escaping(escape)}([\\s\\S]?)|${VARIABLE}`, "g")
  return text.replace(pattern, (match, escaped?: string, ...variable: (string | undefined)[]) => {
    if (escaped === undefined) return fill(variable, lookup, escape)
    return quoted && !['"', "$", escape].includes(escaped) ? match : escaped
  })
}

/** `$name`, `${name}`, `${name:-word}` and `${name-word}`, `${name:+word}` and `${name+word}`. */
function fill(variable: (string | undefined)[], lookup: Lookup, escape: string): string {
  const [braced, colon, op, word = "", bare] = variable
  const value = lookup(braced ?? bare ?? "")
  if (op === undefined) return value ?? UNKNOWN
  const set = colon === ":" ? Boolean(value) : value !== undefined
  return set === (op === "+") ? expand(word, lookup, escape).join(" ") : (value ?? "")
}
