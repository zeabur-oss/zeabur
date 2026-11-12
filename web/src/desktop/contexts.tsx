import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import StarIcon from "~icons/ph/star"
import StarFillIcon from "~icons/ph/star-fill"
import WarningIcon from "~icons/ph/warning"

import { useI18n } from "@/i18n"

import { blocked, type Context, type Reach, type ShellRead } from "./api"
import { authText, fileLabel, reachView } from "./model"

function ReachBadge({ reach }: { reach: Reach | undefined }) {
  const { t } = useI18n()
  const view = reachView(reach, t)
  return <Badge variant={view.variant}>{view.text}</Badge>
}

function FavouriteToggle({ on, onToggle }: { on: boolean; onToggle: () => void }) {
  const { t } = useI18n()
  const label = on ? t("desktop.favouriteRemove") : t("desktop.favouriteAdd")
  return (
    <Tooltip content={label}>
      <Button variant="ghost" size="icon-xs" aria-label={label} onClick={onToggle}>
        {on ? (
          <StarFillIcon className="text-warn" aria-hidden="true" />
        ) : (
          <StarIcon aria-hidden="true" />
        )}
      </Button>
    </Tooltip>
  )
}

function SameName({ count }: { count: number }) {
  const { t } = useI18n()
  if (count === 0) return null
  return (
    <span className="min-w-0 truncate text-warn-strong" title={t("desktop.sameName", count)}>
      {t("desktop.sameName", count)}
    </span>
  )
}

function ContextName({ context }: { context: Context }) {
  const { t } = useI18n()
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 items-center gap-2">
        <span
          className="truncate type-body font-medium text-main"
          title={context.name}
          translate="no"
        >
          {context.name}
        </span>
        {context.current ? <Badge variant="secondary">{t("desktop.current")}</Badge> : null}
      </div>
      <div className="flex min-w-0 items-center gap-2 type-meta text-tertiary">
        <span className="shrink-0" title={context.file} translate="no">
          {fileLabel(context.file)}
        </span>
        <SameName count={context.sameNameElsewhere} />
      </div>
    </div>
  )
}

function Cell({ value, className }: { value: string | null; className: string }) {
  return (
    <div className={className} title={value ?? ""} translate="no">
      {value ?? "-"}
    </div>
  )
}

function ContextServer({ context }: { context: Context }) {
  return (
    <div className="min-w-0">
      <Cell value={context.server} className="truncate type-mono type-meta text-secondary" />
      <Cell value={context.user} className="truncate type-meta text-tertiary" />
    </div>
  )
}

function ContextStatus({
  context,
  reach,
  shell,
}: {
  context: Context
  reach: Reach | undefined
  shell: ShellRead
}) {
  const { t } = useI18n()
  const problem = authText(context.auth, shell, t)
  const tone = blocked(context.auth) ? "text-fail-strong" : "text-warn-strong"
  return (
    <div className="flex min-w-0 flex-col items-start gap-1">
      <ReachBadge reach={reach} />
      {problem ? (
        <span className={`max-w-full truncate type-meta ${tone}`} title={problem}>
          {problem}
        </span>
      ) : null}
    </div>
  )
}

export interface RowActions {
  open: (context: Context) => void
  toggleFavourite: (context: Context) => void
}

interface RowProps {
  context: Context
  favourite: boolean
  reach: Reach | undefined
  shell: ShellRead
  actions: RowActions
}

function ContextRow({ context, favourite, reach, shell, actions }: RowProps) {
  const { t } = useI18n()
  return (
    <li className="grid grid-cols-desktop-row items-center gap-3 border-b border-v2-separator px-4 py-2.5 last:border-b-0">
      <FavouriteToggle on={favourite} onToggle={() => actions.toggleFavourite(context)} />
      <ContextName context={context} />
      <ContextServer context={context} />
      <ContextStatus context={context} reach={reach} shell={shell} />
      <Button
        variant="secondary"
        size="xs"
        disabled={blocked(context.auth)}
        onClick={() => actions.open(context)}
      >
        {t("desktop.open")}
      </Button>
    </li>
  )
}

interface ListProps {
  contexts: Context[]
  favourites: Set<string>
  reach: Record<string, Reach>
  shell: ShellRead
  actions: RowActions
}

function ContextList({ contexts, favourites, reach, shell, actions }: ListProps) {
  return (
    <ul className="rounded-lg border border-v2-border bg-surface">
      {contexts.map((c) => (
        <ContextRow
          key={c.key}
          context={c}
          favourite={favourites.has(c.key)}
          reach={reach[c.key]}
          shell={shell}
          actions={actions}
        />
      ))}
    </ul>
  )
}

function ShellAlert({ shell }: { shell: ShellRead }) {
  const { t } = useI18n()
  if (shell.state !== "failed") return null
  const title = shell.timedOut
    ? t("desktop.shellTimeout", shell.shell)
    : t("desktop.shellFailed", shell.shell)
  return (
    <Alert variant="warn" className="mb-4" title={shell.detail || undefined}>
      <WarningIcon aria-hidden="true" />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>{t("desktop.shellFix")}</AlertDescription>
    </Alert>
  )
}

function Recents({ contexts, ...list }: ListProps) {
  const { t } = useI18n()
  if (contexts.length === 0) return null
  return (
    <section className="mb-5">
      <p className="mb-2 type-label text-tertiary">{t("desktop.recent")}</p>
      <ContextList contexts={contexts} {...list} />
    </section>
  )
}

interface PaneProps extends Omit<ListProps, "contexts"> {
  visible: Context[]
  recents: Context[]
  query: string
  setQuery: (query: string) => void
}

export function ContextPane({ visible, recents, query, setQuery, ...list }: PaneProps) {
  const { t } = useI18n()
  return (
    <main className="flex min-w-0 flex-1 flex-col">
      <header className="flex flex-col gap-3 border-b border-v2-border px-5 pt-4 pb-3">
        <div className="flex items-center gap-3">
          <h1 className="type-title">{t("desktop.title")}</h1>
          <span className="type-meta text-tertiary">{t("desktop.count", visible.length)}</span>
        </div>
        <Input
          placeholder={t("desktop.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={t("desktop.search")}
        />
      </header>
      <div className="flex-1 overflow-y-auto px-5 py-4">
        <ShellAlert shell={list.shell} />
        <Recents contexts={recents} {...list} />
        {visible.length === 0 ? (
          <p className="py-10 text-center type-body text-secondary">
            {t("desktop.noResults", query)}
          </p>
        ) : (
          <ContextList contexts={visible} {...list} />
        )}
        <p className="mt-3 type-meta text-tertiary">{t("desktop.reachNote")}</p>
      </div>
    </main>
  )
}
