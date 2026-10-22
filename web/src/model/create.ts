// Objects written as YAML, read here only to point at lines: the documents, what each names, the
// line of each field and of a syntax error, and the order the server creates them in
// (crates/zeabur-plan/src/create.rs). The server reads the YAML itself; what it refuses comes
// back as a document index and a field or line.
import { yamlLanguage } from "@codemirror/lang-yaml"

import type { Schemas } from "@/api/client"

type Tree = ReturnType<typeof yamlLanguage.parser.parse>
type Node = Tree["topNode"]

export interface YamlDocument {
  /** In written order, counting only documents that hold something. */
  index: number
  /** The document alone, as sent in its `create`. */
  text: string
  /** Its first line in the whole text, from 1. */
  line: number
  apiVersion?: string
  kind?: string
  name?: string
  namespace?: string
  /** Each field's line in the whole text: `metadata.name`, `spec.containers[0].image`. */
  lines: Map<string, number>
  /** The first line the YAML does not read on. */
  error?: number
}

const separator = /^(---|\.\.\.)(\s.*)?$/

/** The documents of `text`, each with its fields' lines. */
export function documentsOf(text: string): YamlDocument[] {
  const lines = text.split("\n")
  const chunks: { start: number; lines: string[] }[] = [{ start: 1, lines: [] }]
  lines.forEach((line, i) => {
    if (separator.test(line)) chunks.push({ start: i + 2, lines: [] })
    else chunks.at(-1)?.lines.push(line)
  })
  const documents: YamlDocument[] = []
  for (const chunk of chunks) {
    const body = chunk.lines.join("\n")
    if (!chunk.lines.some((l) => l.trim() !== "" && !l.trimStart().startsWith("#"))) continue
    documents.push(read(body, chunk.start, documents.length))
  }
  return documents
}

function read(text: string, start: number, index: number): YamlDocument {
  const tree = yamlLanguage.parser.parse(text)
  const lineAt = (offset: number) => start + text.slice(0, offset).split("\n").length - 1
  const lines = new Map<string, number>()
  const values = new Map<string, string>()
  let error: number | undefined
  tree.iterate({
    enter(node) {
      if (node.type.isError && error === undefined) error = lineAt(node.from)
    },
  })
  const scalar = (node: Node) =>
    node.name === "QuotedLiteral"
      ? text.slice(node.from + 1, node.to - 1)
      : node.name === "Literal"
        ? text.slice(node.from, node.to).trim()
        : undefined
  const walk = (node: Node | null, path: string) => {
    if (!node) return
    if (node.name === "BlockMapping" || node.name === "FlowMapping") {
      for (const pair of node.getChildren("Pair")) {
        const key = pair.getChild("Key")?.firstChild
        const name = key ? scalar(key) : undefined
        if (name === undefined) continue
        const at = path ? `${path}.${name}` : name
        lines.set(at, lineAt(pair.from))
        const value = pair.lastChild
        if (!value || value.name === "Key" || value.name === ":") continue
        const plain = scalar(value)
        if (plain !== undefined) values.set(at, plain)
        walk(value, at)
      }
      return
    }
    if (node.name === "BlockSequence" || node.name === "FlowSequence") {
      node.getChildren("Item").forEach((item, i) => {
        const at = `${path}[${i}]`
        lines.set(at, lineAt(item.from))
        walk(item.firstChild, at)
      })
      return
    }
    for (let child = node.firstChild; child; child = child.nextSibling) walk(child, path)
  }
  walk(tree.topNode, "")
  return {
    index,
    text,
    line: start,
    apiVersion: values.get("apiVersion"),
    kind: values.get("kind"),
    name: values.get("metadata.name"),
    namespace: values.get("metadata.namespace"),
    lines,
    error,
  }
}

/** The line of `field` as kube-apiserver writes it (`spec.template.spec.containers[0].image`,
 * `metadata.labels[app]`), or of the nearest field around it; the document's first line when
 * none is written. */
export function lineOf(document: YamlDocument, field: string | undefined): number {
  let path = (field ?? "").replace(/\[([^\]\d][^\]]*)\]/g, ".$1")
  while (path) {
    const line = document.lines.get(path)
    if (line !== undefined) return line
    const cut = Math.max(path.lastIndexOf("."), path.lastIndexOf("["))
    path = cut > 0 ? path.slice(0, cut) : ""
  }
  return document.line
}

const groupOf = (apiVersion = "") => (apiVersion.includes("/") ? apiVersion.split("/")[0] : "")

/** Where a kind comes in the order of creation, as the server orders it. */
function rank(document: YamlDocument): number {
  const group = groupOf(document.apiVersion)
  const kind = document.kind ?? ""
  if (
    group === "" &&
    ["ConfigMap", "Secret", "PersistentVolumeClaim", "ServiceAccount"].includes(kind)
  )
    return 0
  if (group === "" && kind === "Service") return 1
  if (
    (group === "apps" && ["Deployment", "StatefulSet", "DaemonSet", "ReplicaSet"].includes(kind)) ||
    (group === "batch" && ["Job", "CronJob"].includes(kind)) ||
    (group === "" && ["Pod", "ReplicationController"].includes(kind))
  )
    return 2
  return 3
}

/** The documents in the order they are created. */
export function inCreateOrder(documents: readonly YamlDocument[]): YamlDocument[] {
  return documents.toSorted((a, b) => rank(a) - rank(b) || a.index - b.index)
}

/** A problem found before planning, at a line of the whole text. */
export type Found =
  | { code: "invalidYaml"; line: number; document: number }
  | { code: "missingField"; line: number; document: number; field: string }
  | { code: "clusterScoped"; line: number; document: number; kind: string }
  | { code: "notServed"; line: number; document: number; kind: string; apiVersion: string }
  | { code: "rights"; line: number; document: number; kind: string }
  | { code: "otherNamespace"; line: number; document: number; namespace: string }
  | { code: "duplicate"; line: number; document: number; kind: string; name: string }
  | { code: "exists"; line: number; document: number; kind: string; name: string; id: string }

/** Problems that keep a plan from being made; the others are only hints. */
export const blocking = (found: Found) => found.code !== "rights"

export interface Known {
  /** What `GET /creatable` answered; `undefined` while it loads. */
  creatable?: Schemas["CreatableList"]
  /** `<group>/<resource>/<name>` of objects that exist, `core` for the core group. */
  existing?: ReadonlySet<string>
}

/** The kind a document names, as discovery serves it. */
export function servedKind(
  creatable: Schemas["CreatableList"] | undefined,
  document: YamlDocument,
): Schemas["CreatableKind"] | undefined {
  const group = groupOf(document.apiVersion)
  return creatable?.items.find((k) => k.group === group && k.kind === document.kind)
}

/** What the server would refuse, found before asking it, each at its line. */
export function check(documents: readonly YamlDocument[], project: string, known: Known): Found[] {
  const found: Found[] = []
  const seen = new Set<string>()
  for (const d of documents) {
    const document = d.index
    if (d.error !== undefined) {
      found.push({ code: "invalidYaml", line: d.error, document })
      continue
    }
    const missing = (["apiVersion", "kind", "metadata.name"] as const).find(
      (f) => !(f === "apiVersion" ? d.apiVersion : f === "kind" ? d.kind : d.name),
    )
    if (missing) {
      found.push({ code: "missingField", line: lineOf(d, missing), document, field: missing })
      continue
    }
    const kind = d.kind ?? ""
    const group = groupOf(d.apiVersion)
    const kindLine = lineOf(d, "kind")
    const served = servedKind(known.creatable, d)
    if (known.creatable?.clusterScoped.some((k) => k.group === group && k.kind === kind))
      found.push({ code: "clusterScoped", line: kindLine, document, kind })
    else if (known.creatable && !served)
      found.push({
        code: "notServed",
        line: kindLine,
        document,
        kind,
        apiVersion: d.apiVersion ?? "",
      })
    else if (served?.access === "denied")
      found.push({ code: "rights", line: kindLine, document, kind })
    if (d.namespace && d.namespace !== project)
      found.push({
        code: "otherNamespace",
        line: lineOf(d, "metadata.namespace"),
        document,
        namespace: d.namespace,
      })
    const name = d.name ?? ""
    const key = `${group}/${kind}/${name}`
    if (seen.has(key))
      found.push({ code: "duplicate", line: lineOf(d, "metadata.name"), document, kind, name })
    seen.add(key)
    if (served) {
      const id = [served.group || "core", served.version, served.resource, name].join("/")
      if (known.existing?.has([served.group || "core", served.resource, name].join("/")))
        found.push({ code: "exists", line: lineOf(d, "metadata.name"), document, kind, name, id })
    }
  }
  return found
}

/** The changes that create the documents, one per document, in written order. */
export function creates(documents: readonly YamlDocument[]): Schemas["Change"][] {
  return documents.map((d) => ({ op: "create", yaml: d.text }))
}
