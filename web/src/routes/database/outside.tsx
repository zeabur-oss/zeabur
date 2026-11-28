import { Link } from "@tanstack/react-router"

import { CopyText } from "@/components/copy-text"
import { DocsLink } from "@/components/docs-link"
import { useGate, useGateReason } from "@/components/gate"
import { CardRow, CardRows, Section } from "@/components/store-detail"
import { useT } from "@/i18n"
import { connectCommand, type Database } from "@/model/databases"

/** How to reach the database from outside the cluster: `zeaburctl connect`. */
export function Outside({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const reason = useGateReason()(useGate(project, "portForward"))
  const command = connectCommand(project, database)
  return (
    <Section id="db-outside" title={t("db.outside.title")}>
      <CardRows>
        <CardRow label="zeaburctl">
          <span className="min-w-0 truncate type-mono" translate="no">
            {command}
          </span>
          <CopyText value={() => command} label={t("common.copy")} />
        </CardRow>
      </CardRows>
      <ol className="flex list-decimal flex-col gap-1.5 pl-5 type-meta text-muted-foreground">
        <li>
          {t("db.outside.install")}{" "}
          <DocsLink page="reference/zeaburctl">{t("db.outside.docs")}</DocsLink>
        </li>
        <li>
          {t("db.outside.kubeconfig")}{" "}
          <Link to="/members" className="text-foreground underline underline-offset-2">
            {t("db.outside.tokens")}
          </Link>
        </li>
        <li>{t("db.outside.run")}</li>
        <li>{t("db.outside.client")}</li>
      </ol>
      {reason ? (
        <p className="type-meta text-warn-strong">{t("db.outside.refused", reason)}</p>
      ) : null}
      <DocsLink page="reference/zeaburctl" anchor="connect">
        {t("db.outside.reference")}
      </DocsLink>
    </Section>
  )
}
