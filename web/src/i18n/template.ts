// The server sentences' templates (crates/zeabur-gaps/messages): `${name}`, and `${name, kind, …}`
// with cases in braces. `zeabur_gaps::messages` renders the same templates the same way; the
// format is described in docs/internal/wording.md.

/** A sentence's parameters, by name. */
export type Params = Record<string, string>

/** What a language joins lists and writes durations with. */
export interface Format {
  listSeparator: string
  listAnd: string
  seconds: string
  minutes: string
}

type Node = string | { hash: true } | Placeholder

interface Placeholder {
  name: string
  kind: string
  style: string
  cases: Map<string, Node[]>
}

class Parser {
  private at = 0
  private readonly source: string

  constructor(source: string) {
    this.source = source
  }

  /** The nodes up to the end, or up to the `}` that closes a case. */
  nodes(inCase: boolean, hash: boolean): Node[] {
    const nodes: Node[] = []
    let text = ""
    const flush = () => {
      if (text) nodes.push(text)
      text = ""
    }
    while (this.at < this.source.length) {
      const c = this.source[this.at]!
      if (c === "$" && this.source[this.at + 1] === "{") {
        flush()
        this.at += 2
        nodes.push(this.placeholder(hash))
      } else if (c === "}" && inCase) {
        flush()
        return nodes
      } else if (c === "#" && hash) {
        flush()
        nodes.push({ hash: true })
        this.at += 1
      } else {
        text += c
        this.at += 1
      }
    }
    if (inCase) throw new Error(`unclosed case in ${this.source}`)
    flush()
    return nodes
  }

  private word(): string {
    this.space()
    const start = this.at
    while (this.at < this.source.length && !/[\s,{}]/.test(this.source[this.at]!)) this.at += 1
    return this.source.slice(start, this.at)
  }

  private space() {
    while (/\s/.test(this.source[this.at] ?? "")) this.at += 1
  }

  private placeholder(hash: boolean): Placeholder {
    const name = this.word()
    const placeholder: Placeholder = { name, kind: "", style: "", cases: new Map() }
    this.space()
    if (this.source[this.at] === ",") {
      this.at += 1
      placeholder.kind = this.word()
      const caseHash = placeholder.kind === "plural" ? true : hash
      for (;;) {
        this.space()
        if (this.source[this.at] === ",") this.at += 1
        const key = this.word()
        if (!key) break
        this.space()
        if (this.source[this.at] === "{") {
          this.at += 1
          placeholder.cases.set(key, this.nodes(true, caseHash))
          this.at += 1
        } else placeholder.style = key
      }
    }
    this.space()
    if (this.source[this.at] !== "}") throw new Error(`unclosed placeholder in ${this.source}`)
    this.at += 1
    return placeholder
  }
}

/** Parses a template; throws on one that is not well-formed. */
export function parse(source: string): Node[] {
  return new Parser(source).nodes(false, false)
}

/** A number as JavaScript writes it. */
const number = (n: number) => String(n)

function write(nodes: Node[], params: Params, format: Format, hash: string): string {
  return nodes.map((node) => node_(node, params, format, hash)).join("")
}

function pick(cases: Map<string, Node[]>, key: string): Node[] {
  return cases.get(key) ?? cases.get("other") ?? []
}

function node_(node: Node, params: Params, format: Format, hash: string): string {
  if (typeof node === "string") return node
  if ("hash" in node) return hash
  const value = params[node.name]
  const items = (value ?? "").split(",")
  switch (node.kind) {
    case "":
      return value ?? ""
    case "select":
      return write(
        pick(node.cases, value ? value : node.cases.has("none") ? "none" : "other"),
        params,
        format,
        hash,
      )
    case "plural":
      return write(
        pick(node.cases, Number(value) === 1 ? "one" : "other"),
        params,
        format,
        value ?? "",
      )
    case "items":
      return write(pick(node.cases, items.length === 1 ? "one" : "other"), params, format, hash)
    case "first":
      return write(pick(node.cases, items[0]!), params, format, hash)
    case "list": {
      const words =
        node.cases.size === 0
          ? items
          : items.flatMap((item) => {
              const chosen = node.cases.get(item) ?? node.cases.get("other")
              return chosen ? [write(chosen, params, format, item)] : []
            })
      return node.style === "and" && words.length > 1
        ? `${words.slice(0, -1).join(format.listSeparator)}${format.listAnd}${words.at(-1)}`
        : words.join(format.listSeparator)
    }
    case "duration": {
      const seconds = Number(value)
      return seconds >= 60 && seconds % 60 === 0
        ? render(format.minutes, { n: number(seconds / 60) }, format)
        : render(format.seconds, { n: number(seconds) }, format)
    }
    default:
      throw new Error(`unknown placeholder kind ${node.kind}`)
  }
}

const parsed = new Map<string, Node[]>()

/** `template` with `params`. */
export function render(template: string, params: Params, format: Format): string {
  let nodes = parsed.get(template)
  if (!nodes) {
    nodes = parse(template)
    parsed.set(template, nodes)
  }
  return write(nodes, params, format, "")
}

/** A catalog's sentences as the dictionaries hold them: `sentence.<key>`, each taking the
 * sentence's parameters. */
export function sentences<const M extends Record<string, string>>(catalog: {
  format: Format
  messages: M
}): { [K in keyof M & string as `sentence.${K}`]: (p: Params) => string } {
  return Object.fromEntries(
    Object.entries(catalog.messages).map(([key, template]) => [
      `sentence.${key}`,
      (p: Params) => render(template, p, catalog.format),
    ]),
  ) as { [K in keyof M & string as `sentence.${K}`]: (p: Params) => string }
}
