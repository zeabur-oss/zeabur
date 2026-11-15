import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import WarningIcon from "~icons/ph/warning"

import { useI18n } from "@/i18n"

import { blocked, type Context, type ShellRead } from "./api"
import { authText, fileLabel } from "./model"

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
      <div className="min-w-0 truncate type-meta text-tertiary" title={context.file} translate="no">
        {fileLabel(context.file)}
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
    <div className="min-w-0 @max-2xl:col-start-1 @max-2xl:row-start-2">
      <Cell value={context.server} className="truncate type-mono type-meta text-secondary" />
      <Cell value={context.user} className="truncate type-meta text-tertiary" />
    </div>
  )
}

function ContextStatus({ context, shell }: { context: Context; shell: ShellRead }) {
  const { t } = useI18n()
  const problem = authText(context.auth, shell, t)
  const tone = blocked(context.auth) ? "text-fail-strong" : "text-warn-strong"
  return (
    <div className="min-w-0 @max-2xl:col-start-2 @max-2xl:row-span-2 @max-2xl:row-start-1">
      {problem ? (
        <span className={`block truncate type-meta ${tone}`} title={problem}>
          {problem}
        </span>
      ) : null}
    </div>
  )
}

export interface RowActions {
  open: (context: Context) => void
}

interface RowProps {
  context: Context
  shell: ShellRead
  actions: RowActions
}

function ContextRow({ context, shell, actions }: RowProps) {
  const { t } = useI18n()
  return (
    <li className="grid grid-cols-desktop-row items-center gap-x-3 gap-y-1 border-b border-v2-separator px-4 py-2.5 last:border-b-0 @max-2xl:grid-cols-desktop-row-narrow">
      <ContextName context={context} />
      <ContextServer context={context} />
      <ContextStatus context={context} shell={shell} />
      <Button
        variant="secondary"
        size="xs"
        className="@max-2xl:col-start-3 @max-2xl:row-span-2 @max-2xl:row-start-1"
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
  shell: ShellRead
  actions: RowActions
}

function ContextList({ contexts, shell, actions }: ListProps) {
  return (
    <ul className="@container rounded-lg border border-v2-border bg-surface">
      {contexts.map((c) => (
        <ContextRow key={c.key} context={c} shell={shell} actions={actions} />
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

interface PaneProps extends Omit<ListProps, "contexts"> {
  visible: Context[]
  query: string
  setQuery: (query: string) => void
}

export function ContextPane({ visible, query, setQuery, ...list }: PaneProps) {
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
        {visible.length === 0 ? (
          <p className="py-10 text-center type-body text-secondary">
            {t("desktop.noResults", query)}
          </p>
        ) : (
          <ContextList contexts={visible} shell={list.shell} actions={list.actions} />
        )}
      </div>
    </main>
  )
}
