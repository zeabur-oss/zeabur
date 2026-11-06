import { useId } from "react"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { CopyText } from "@/components/copy-text"
import { DocsLink } from "@/components/docs-link"
import { GateTip, useGate, useGateReason } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { CardRow, CardRows, Section } from "@/components/store-detail"
import { useT } from "@/i18n"
import {
  connectCommand,
  POSTGRES_PORT,
  publicAddresses,
  publicUri,
  unavailableReason,
  type Database,
} from "@/model/databases"
import { useProjectEvents } from "@/routes/project/data"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Label } from "@zeabur-oss/design-system/components/label"
import { Switch } from "@zeabur-oss/design-system/components/switch"

const mono = (text: string) => (
  <span className="min-w-0 truncate type-mono" translate="no">
    {text}
  </span>
)

/** How to reach the database from outside the cluster: zeaburctl for every engine, and a
 * public port for PostgreSQL. */
export function Outside({ project, database, credentials }: Props) {
  const t = useT()
  const forward = useGate(project, "portForward")
  const reason = useGateReason()(forward)
  const command = connectCommand(project, database)
  return (
    <Section id="db-outside" title={t("db.outside.title")}>
      <CardRows>
        <CardRow label="zeaburctl">
          {mono(command)}
          <CopyText value={() => command} label={t("common.copy")} />
        </CardRow>
      </CardRows>
      <p className="type-meta text-muted-foreground">{t("db.outside.commandHint")}</p>
      {reason ? (
        <p className="type-meta text-warn-strong">{t("db.outside.refused", reason)}</p>
      ) : null}
      <DocsLink page="reference/zeaburctl">{t("db.outside.docs")}</DocsLink>
      {database.engine === "postgresql" ? (
        <PublicPort project={project} database={database} credentials={credentials} />
      ) : null}
    </Section>
  )
}

type Credentials = { username?: string; password?: string }

interface Props {
  project: string
  database: Database
  /** What the person revealed in the in-cluster connection, for the public connection string. */
  credentials: Credentials
}

function PublicPort({ project, database, credentials }: Props) {
  const t = useT()
  const id = useId()
  const gate = useGate(project, "changeDomains")
  const planner = usePlanner(project)
  return (
    <div className="flex flex-col gap-3 border-t pt-3">
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <Label htmlFor={id}>{t("db.outside.public")}</Label>
          <p className="type-meta text-muted-foreground">
            {t("db.outside.publicHint", POSTGRES_PORT)}
          </p>
        </div>
        <GateTip gate={gate}>
          <Switch
            id={id}
            checked={Boolean(database.publicPort)}
            onCheckedChange={(on) =>
              planner.plan({
                title: on
                  ? t("ports.openTitle", POSTGRES_PORT, database.name)
                  : t("ports.closeTitle", POSTGRES_PORT, database.name),
                changes: [{ op: "databasePublicPort", database: database.name, remove: !on }],
              })
            }
          />
        </GateTip>
      </div>
      {database.publicPort ? (
        <>
          <Alert variant="warn">
            <WarningIcon aria-hidden="true" />
            <AlertTitle>{t("ports.databaseTitle", database.name)}</AlertTitle>
            <AlertDescription>{t("ports.databaseBody")}</AlertDescription>
          </Alert>
          <Addresses
            project={project}
            database={database}
            port={database.publicPort}
            credentials={credentials}
          />
        </>
      ) : null}
      {planner.dialog}
    </div>
  )
}

function Waiting({ project, port }: { project: string; port: Schemas.DatabasePublicPort }) {
  const t = useT()
  const reason = unavailableReason(useProjectEvents(project), port.service)
  return (
    <p className="type-meta text-muted-foreground">
      {t("ports.waiting", port.port)}
      {reason ? ` ${reason}` : null}
    </p>
  )
}

function Addresses({
  project,
  database,
  port,
  credentials,
}: {
  project: string
  database: Database
  port: Schemas.DatabasePublicPort
  credentials: Credentials
}) {
  const t = useT()
  const { first, others, allPrivate } = publicAddresses(port)
  if (!first) return <Waiting project={project} port={port} />
  const connection = { ...database.connection, username: credentials.username }
  const uri = publicUri({ ...database, connection }, first, credentials.password)
  const address = `${first}:${port.port}`
  return (
    <>
      <CardRows>
        <CardRow label={t("db.outside.address")}>
          {mono(address)}
          <CopyText value={() => address} label={t("common.copy")} />
        </CardRow>
        <CardRow label={t("db.uri")}>
          {mono(uri)}
          <CopyText value={() => uri} label={t("common.copy")} />
        </CardRow>
        {others.length > 0 ? (
          <CardRow label={t("db.outside.others")}>
            {mono(others.map((a) => `${a}:${port.port}`).join(", "))}
          </CardRow>
        ) : null}
      </CardRows>
      {allPrivate ? (
        <p className="type-meta text-muted-foreground">{t("db.outside.private", port.port)}</p>
      ) : null}
    </>
  )
}
