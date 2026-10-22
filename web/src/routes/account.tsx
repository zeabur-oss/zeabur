import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { accountQuery } from "@/api/members"
import { Panel, Row, Rows } from "@/components/bits"
import { FilterField } from "@/components/filter-field"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import {
  accessCounts,
  accessLayout,
  accessRows,
  filterAccess,
  projectName,
  roles,
  type ProjectAccess,
} from "@/model/account"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Badge } from "@zeabur/design-system/components/badge"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { Empty, EmptyHeader, EmptyTitle } from "@zeabur/design-system/components/empty"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Skeleton } from "@zeabur/design-system/components/skeleton"

type Account = Schemas["AccountView"]

export function AccountPage() {
  const t = useT()
  const account = useQuery(accountQuery)
  return (
    <PageColumn width="content" gap="md" className="my-6 md:my-8">
      <PageHeader title={t("account.title")} />
      {account.isPending ? (
        <Skeleton variant="card" className="h-40" />
      ) : account.isError ? (
        <Problem error={account.error} what={t("refused.see", t("account.yours"))} />
      ) : (
        <>
          <You account={account.data} />
          <Panel title={t("account.access")}>
            <Access account={account.data} />
          </Panel>
          {account.data.manageSignInUrl ? <SignIn url={account.data.manageSignInUrl} /> : null}
        </>
      )}
    </PageColumn>
  )
}

function You({ account }: { account: Account }) {
  const t = useT()
  return (
    <Panel title={t("account.you")}>
      <Rows>
        <Row label={t("account.username")}>
          <span className="font-mono text-xs" translate="no">
            {account.username}
          </span>
        </Row>
        {account.groups.length > 0 ? (
          <Row label={t("account.groups")}>
            <span className="flex flex-wrap gap-1.5">
              {account.groups.map((group) => (
                <span
                  key={group}
                  className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-xs"
                  translate="no"
                >
                  {group}
                </span>
              ))}
            </span>
          </Row>
        ) : null}
      </Rows>
    </Panel>
  )
}

function Note({ children }: { children: string }) {
  return <p className="text-sm text-muted-foreground">{children}</p>
}

function Access({ account }: { account: Account }) {
  const t = useT()
  const projects = account.projects
  if (account.clusterAdmin) return <Note>{t("account.clusterAdmin")}</Note>
  if (!projects) return <Note>{t("account.unlisted")}</Note>
  if (projects.length === 0)
    return (
      <Empty size="inline">
        <EmptyHeader>
          <EmptyTitle>{t("account.noProjects")}</EmptyTitle>
        </EmptyHeader>
      </Empty>
    )
  const counts = accessCounts(projects)
  const { rows, without } = accessRows(projects)
  const held = roles.filter((role) => counts[role] > 0)
  return (
    <div className="flex flex-col gap-4">
      {counts.unreadable > 0 ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{t("account.unreadCount", counts.unreadable)}</AlertDescription>
        </Alert>
      ) : null}
      {held.length > 0 ? (
        <p className="text-sm">
          {t(
            "account.summary",
            held.map((role) => t("account.roleIn", t(`account.role.${role}`), counts[role])),
          )}
        </p>
      ) : null}
      {rows.length > 0 ? <AccessList rows={rows} /> : null}
      {without.count > 0 ? (
        <Note>
          {rows.length === 0
            ? t("account.noRoleCount", without.count, true)
            : without.named.length > 0
              ? t("account.noRole", without.named)
              : t("account.noRoleCount", without.count, false)}
        </Note>
      ) : null}
      {held.length > 0 ? (
        <dl className="flex flex-col gap-1 border-t border-main pt-3 type-meta text-muted-foreground">
          {held.map((role) => (
            <div key={role} className="flex flex-wrap gap-x-2">
              <dt className="font-medium text-secondary">{t(`account.role.${role}`)}</dt>
              <dd>{t(`account.means.${role}`)}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </div>
  )
}

function AccessList({ rows }: { rows: ProjectAccess[] }) {
  const t = useT()
  const [query, setQuery] = useState("")
  const table = accessLayout(rows.length) === "table"
  const shown = table ? filterAccess(rows, query) : rows
  return (
    <div className="flex flex-col gap-3">
      {table ? <FilterField value={query} onChange={setQuery} label={t("account.filter")} /> : null}
      <div
        className={cn(
          "rounded-xl border border-main text-sm",
          table && "max-h-96 overflow-y-auto overscroll-contain",
        )}
      >
        {shown.length === 0 ? (
          <p className="px-4 py-3 text-muted-foreground">{t("account.noMatch")}</p>
        ) : (
          <ul className="divide-y divide-main">
            {shown.map((project) => (
              <li key={project.name} className="flex min-w-0 items-center gap-3 px-4 py-2.5">
                <Link
                  to="/projects/$project"
                  params={{ project: project.name }}
                  className="min-w-0 flex-1 truncate font-medium hover:underline"
                  translate="no"
                >
                  {projectName(project)}
                </Link>
                {project.role && project.role !== "none" ? (
                  <Badge variant="secondary">{t(`account.role.${project.role}`)}</Badge>
                ) : (
                  <Badge variant="warn">{t("account.unread")}</Badge>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

function SignIn({ url }: { url: string }) {
  const t = useT()
  return (
    <Panel title={t("account.signIn")}>
      <div className="flex flex-col items-start gap-3">
        <Note>{t("account.signInWhere")}</Note>
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          className={cn(buttonVariants({ size: "sm", variant: "outline" }))}
        >
          {t("account.manageSignIn")}
          <ArrowSquareOutIcon aria-hidden="true" data-icon="inline-end" />
          <span className="sr-only">{t("common.newTab")}</span>
        </a>
      </div>
    </Panel>
  )
}
