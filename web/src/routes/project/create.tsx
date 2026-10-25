// Objects written as YAML, created through a plan: the editor with templates, what the server
// would refuse found before planning and pointed at its line, the order of creation beside it, and
// after a refusal part way, what was created with a plan to delete exactly that.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"
import CheckIcon from "~icons/ph/check"
import PlusIcon from "~icons/ph/plus"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import type { Problem as ProblemBody, Schemas } from "@/api/client"
import { creatableQuery, objectsQuery } from "@/api/queries"
import { Hint } from "@/components/bits"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import type { LineMark, YamlEditorHandle } from "@/components/yaml-editor"
import { useT, type Translate } from "@/i18n"
import { cn } from "@/lib/utils"
import {
  blocking,
  check,
  creates,
  documentsOf,
  inCreateOrder,
  lineOf,
  servedKind,
  type Found,
  type YamlDocument,
} from "@/model/create"
import { deletionOf } from "@/model/risk"
import { dropHandoff, handedOff } from "@/lib/adding"
import { appended, offered, templateOf } from "@/model/templates"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Dialog, DialogContent, DialogTitle } from "@zeabur/design-system/components/dialog"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"

const YamlEditor = lazy(() =>
  import("@/components/yaml-editor").then((m) => ({ default: m.YamlEditor })),
)

const route = getRouteApi("/projects/$project/create")

const deployment: Schemas["CreatableKind"] = {
  group: "apps",
  version: "v1",
  resource: "deployments",
  kind: "Deployment",
  custom: false,
  access: "unknown",
}

const keyOf = (group: string, resource: string) => `${group || "core"}/${resource}`

function foundText(t: Translate, found: Found, project: string): string {
  switch (found.code) {
    case "invalidYaml":
      return t("create.found.invalidYaml")
    case "missingField":
      return t("create.found.missingField", found.field)
    case "clusterScoped":
      return t("create.found.clusterScoped", found.kind)
    case "notServed":
      return t("create.found.notServed", found.kind, found.apiVersion)
    case "rights":
      return t("create.found.rights", found.kind, project)
    case "otherNamespace":
      return t("create.found.otherNamespace", found.namespace, project)
    case "duplicate":
      return t("create.found.duplicate", found.kind, found.name)
    case "exists":
      return t("create.found.exists", found.kind, found.name)
  }
}

export function CreatePage() {
  const { project } = route.useParams()
  const t = useT()
  const navigate = useNavigate()
  const creatable = useQuery(creatableQuery(project))
  const [initial] = useState(() => handedOff(project) ?? templateOf(deployment))
  // Dropped once shown: a reload after that starts from the template.
  useEffect(() => dropHandoff(project), [project])
  const [text, setText] = useState(initial)
  const docs = useMemo(() => documentsOf(text), [text])
  const kinds = useMemo(() => {
    const keys = new Set<string>()
    for (const d of docs) {
      const served = servedKind(creatable.data, d)
      if (served) keys.add(keyOf(served.group, served.resource))
    }
    return [...keys].toSorted()
  }, [docs, creatable.data])
  const objects = useQuery({ ...objectsQuery(project, { kinds }), enabled: kinds.length > 0 })
  const existing = useMemo(() => {
    const names = new Set<string>()
    for (const kind of objects.data?.items ?? [])
      for (const item of kind.items) names.add(`${keyOf(kind.group, kind.resource)}/${item.name}`)
    return names
  }, [objects.data])
  const found = check(docs, project, { creatable: creatable.data, existing })
  const stopped = found.some(blocking)

  const editor = useRef<YamlEditorHandle | undefined>(undefined)
  const jump = (line: number) => editor.current?.jump(line)
  const [reviewed, setReviewed] = useState<string | undefined>(undefined)
  const [partial, setPartial] = useState<ProblemBody | undefined>(undefined)
  const toObjects = () => void navigate({ to: "/projects/$project/objects", params: { project } })
  const lineAt = (at: Schemas["ProblemAt"]) => {
    const doc = docs[at.change]
    if (!doc) return undefined
    return at.line ? doc.line + at.line - 1 : lineOf(doc, at.field ?? undefined)
  }
  const planner = usePlanner(project, toObjects, true, {
    where: (at) => {
      const line = lineAt(at)
      if (line === undefined) return ""
      return at.field
        ? t("create.docLineField", at.change + 1, line, at.field)
        : t("create.docLine", at.change + 1, line)
    },
    onPartial: setPartial,
  })
  const refusedAt = reviewed === text ? planner.refusal?.at : undefined
  const refusedLine = refusedAt ? lineAt(refusedAt) : undefined
  useEffect(() => {
    if (refusedLine !== undefined) editor.current?.jump(refusedLine)
  }, [refusedLine])

  const marks: LineMark[] = found.map((f) => ({
    line: f.line,
    tone: blocking(f) ? "fail" : "warn",
    message: foundText(t, f, project),
  }))
  if (refusedLine !== undefined && planner.refusal)
    marks.push({ line: refusedLine, tone: "fail", message: planner.refusal.title })

  const review = () => {
    const ordered = inCreateOrder(docs)
    const [only] = docs
    setReviewed(text)
    planner.plan({
      title:
        docs.length === 1 && only
          ? t("create.planOne", only.kind ?? "", only.name ?? "")
          : t("create.planMany", ordered.length, project),
      changes: creates(docs),
    })
  }

  return (
    <div className="flex flex-col">
      <header className="shrink-0 pt-6 pb-4 md:pt-8">
        <PageColumn>
          <PageHeader title={t("create.title", project)} />
        </PageColumn>
      </header>
      <PageColumn gap="sm" className="mb-20">
        <div className="flex flex-wrap items-center gap-2">
          <Templates
            creatable={creatable.data}
            onAdd={(kind) => {
              const next = appended(text, templateOf(kind))
              editor.current?.replace(next)
              setText(next)
              window.setTimeout(() => jump(next.split("\n").length - 1), 0)
            }}
          />
          <span className="ml-auto type-meta text-muted-foreground tabular-nums">
            {t("create.documents", docs.length)}
          </span>
        </div>
        <div className="flex gap-4">
          <div className="flex min-w-0 flex-1 flex-col gap-3">
            <div className="flex min-h-96 flex-col">
              <Suspense fallback={null}>
                <YamlEditor
                  initial={initial}
                  label={t("create.editor")}
                  onChange={setText}
                  marks={marks}
                  onReady={(handle) => {
                    editor.current = handle
                  }}
                />
              </Suspense>
            </div>
            {docs.length === 0 ? (
              <p className="type-meta text-muted-foreground">{t("create.empty")}</p>
            ) : null}
            {found.length > 0 ? <FoundList project={project} found={found} jump={jump} /> : null}
          </div>
          <Order docs={docs} found={found} jump={jump} />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <span className="type-meta text-muted-foreground">
            {stopped
              ? t("create.fixFirst")
              : found.length > 0
                ? t("create.notes", found.length)
                : null}
          </span>
          <span className="ml-auto flex flex-wrap items-center justify-end gap-2">
            <Link
              to="/projects/$project/objects"
              params={{ project }}
              className={cn(buttonVariants({ variant: "ghost", size: "sm" }))}
            >
              {t("plan.cancel")}
            </Link>
            <Button size="sm" disabled={stopped || docs.length === 0} onClick={review}>
              {t("create.review")}
              <CaretRightIcon aria-hidden="true" data-icon="inline-end" />
            </Button>
          </span>
        </div>
      </PageColumn>
      {planner.dialog}
      {partial ? (
        <Partial
          problem={partial}
          docs={docs}
          onKeep={() => {
            setPartial(undefined)
            toObjects()
          }}
          onDelete={(changes) => {
            setPartial(undefined)
            planner.plan({ title: t("create.cleanupTitle", changes.length), changes })
          }}
        />
      ) : null}
    </div>
  )
}

function Templates({
  creatable,
  onAdd,
}: {
  creatable: Schemas["CreatableList"] | undefined
  onAdd: (kind: Schemas["CreatableKind"]) => void
}) {
  const t = useT()
  const groups = offered(creatable)
  const sections = [
    { label: t("create.common"), kinds: groups.common },
    { label: t("create.custom"), kinds: groups.custom },
    { label: t("create.more"), kinds: groups.more },
  ].filter((s) => s.kinds.length > 0)
  return (
    <Menu>
      <MenuTrigger render={<Button variant="outline" size="sm" disabled={sections.length === 0} />}>
        <PlusIcon aria-hidden="true" data-icon="inline-start" />
        {t("create.templateAdd")}
        <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
      </MenuTrigger>
      <MenuContent align="start" className="max-h-96 overflow-y-auto">
        {sections.map((section, i) => (
          <div key={section.label}>
            {i > 0 ? <MenuSeparator /> : null}
            <MenuLabel>{section.label}</MenuLabel>
            {section.kinds.map((kind) => (
              <MenuItem key={`${kind.group}/${kind.kind}`} onClick={() => onAdd(kind)}>
                <span translate="no">{kind.kind}</span>
                {kind.group ? (
                  <span className="ml-auto type-meta text-muted-foreground" translate="no">
                    {kind.group}
                  </span>
                ) : null}
              </MenuItem>
            ))}
          </div>
        ))}
      </MenuContent>
    </Menu>
  )
}

function FoundList({
  project,
  found,
  jump,
}: {
  project: string
  found: Found[]
  jump: (line: number) => void
}) {
  const t = useT()
  return (
    <ul
      className="flex shrink-0 flex-col rounded-xl border bg-card"
      aria-label={t("create.problems", found.length)}
    >
      {found.map((f) => (
        <li
          key={`${f.line}/${f.code}`}
          className="flex flex-wrap items-start gap-x-3 gap-y-1 border-b px-4 py-2.5 text-sm last:border-b-0"
        >
          <StatusDot health={blocking(f) ? "fail" : "warn"} className="mt-1.5" />
          <span className="min-w-0 flex-1">{foundText(t, f, project)}</span>
          <span className="flex items-center gap-2">
            <Button variant="ghost" size="xs" onClick={() => jump(f.line)}>
              {t("create.docLine", f.document + 1, f.line)}
            </Button>
            {f.code === "exists" ? (
              <Link
                to="/projects/$project/objects"
                params={{ project }}
                search={{ object: f.id }}
                className={cn(buttonVariants({ variant: "outline", size: "xs" }))}
              >
                {t("create.edit", f.name)}
              </Link>
            ) : null}
          </span>
        </li>
      ))}
    </ul>
  )
}

function Order({
  docs,
  found,
  jump,
}: {
  docs: YamlDocument[]
  found: Found[]
  jump: (line: number) => void
}) {
  const t = useT()
  const toneOf = (d: YamlDocument) => {
    const own = found.filter((f) => f.document === d.index)
    return own.some(blocking) ? "fail" : own.length > 0 ? "warn" : undefined
  }
  return (
    <aside className="hidden w-64 shrink-0 flex-col self-start rounded-xl border bg-card lg:flex">
      <header className="flex h-10 items-center gap-1.5 border-b px-4">
        <h2 className="text-sm font-medium">{t("create.inOrder")}</h2>
        <Hint label={t("create.inOrder")}>{t("create.inOrderHint")}</Hint>
      </header>
      <ol className="flex flex-col py-1">
        {inCreateOrder(docs).map((d, i) => {
          const tone = toneOf(d)
          return (
            <li key={d.index}>
              <button
                type="button"
                onClick={() => jump(d.line)}
                className="flex w-full items-center gap-2 px-4 py-1.5 text-left outline-none hover:bg-v2-surface-hover focus-visible:bg-v2-surface-hover"
              >
                <span className="w-4 shrink-0 type-meta text-muted-foreground tabular-nums">
                  {i + 1}
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="type-meta text-muted-foreground" translate="no">
                    {d.kind ?? "?"}
                  </span>
                  <span className="truncate font-mono text-xs" translate="no">
                    {d.name ?? "?"}
                  </span>
                </span>
                {tone ? <StatusDot health={tone} /> : null}
              </button>
            </li>
          )
        })}
      </ol>
    </aside>
  )
}

/** A create refused part way: what was created, what was refused, what was not tried. */
function Partial({
  problem,
  docs,
  onKeep,
  onDelete,
}: {
  problem: ProblemBody
  docs: YamlDocument[]
  onKeep: () => void
  onDelete: (changes: Schemas["Change"][]) => void
}) {
  const t = useT()
  const applied = problem.applied ?? []
  const ordered = inCreateOrder(docs)
  const refused = problem.at?.change
  const created = (d: YamlDocument) => applied.some((a) => a.kind === d.kind && a.name === d.name)
  const deletes = applied
    .map((a) => deletionOf(a))
    .filter((c): c is Schemas["Change"] => c !== undefined)
    .toReversed()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onKeep())}>
      <DialogContent variant="wide" aria-describedby={undefined}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{t("create.partialTitle", applied.length, ordered.length)}</DialogTitle>
          </div>
          <Button variant="ghost" size="icon-sm" aria-label={t("common.close")} onClick={onKeep}>
            <XIcon aria-hidden="true" />
          </Button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 py-4">
          <Alert variant="warn" role="note">
            <WarningFillIcon aria-hidden="true" />
            <AlertDescription>{t("create.noTransaction")}</AlertDescription>
          </Alert>
          <ol className="flex flex-col rounded-xl border bg-card">
            {ordered.map((d) => {
              const mark = created(d) ? "ok" : d.index === refused ? "fail" : "idle"
              return (
                <li
                  key={d.index}
                  className="flex items-center gap-3 border-b px-4 py-2 text-sm last:border-b-0"
                >
                  {mark === "ok" ? (
                    <CheckIcon aria-hidden="true" className="size-4 text-ok" />
                  ) : mark === "fail" ? (
                    <XIcon aria-hidden="true" className="size-4 text-fail" />
                  ) : (
                    <span className="flex size-4 items-center justify-center" aria-hidden="true">
                      <StatusDot health="idle" />
                    </span>
                  )}
                  <span className="w-40 shrink-0" translate="no">
                    {d.kind}
                  </span>
                  <span
                    className={cn(
                      "min-w-0 flex-1 truncate font-mono text-xs",
                      mark === "idle" && "text-muted-foreground",
                    )}
                    translate="no"
                  >
                    {d.name}
                  </span>
                  {mark === "idle" ? (
                    <span className="type-meta text-muted-foreground">
                      {t("create.notAttempted")}
                    </span>
                  ) : null}
                </li>
              )
            })}
          </ol>
          <Problem problem={problem} />
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t px-5 py-3">
          <Button variant="ghost" size="sm" onClick={onKeep}>
            {t("create.keep")}
          </Button>
          {deletes.length > 0 ? (
            <Button variant="destructive" size="sm" onClick={() => onDelete(deletes)}>
              {t("create.deleteCreated", deletes.length)}
            </Button>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
