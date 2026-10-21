import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import PlusIcon from "~icons/ph/plus"
import ShieldWarningIcon from "~icons/ph/shield-warning"

import { accountQuery, createOwnSetupLink, refreshMembers, type SetupLink } from "@/api/members"
import { Panel, Row, Rows } from "@/components/bits"
import { LinkedList, PasskeyList } from "@/components/members/credentials"
import { LinksUnavailable, ManageDenied, OwnLinkDialog } from "@/components/members/dialogs"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import { linkState, memberName, needsBackup, type Member } from "@/model/members"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Skeleton } from "@zeabur/design-system/components/skeleton"

type Account = NonNullable<ReturnType<typeof useAccount>["data"]>

function useAccount() {
  return useQuery(accountQuery)
}

export function AccountPage() {
  const t = useT()
  const account = useAccount()
  return (
    <PageColumn width="content" gap="md" className="my-6 md:my-8">
      <PageHeader title={t("account.title")} />
      {account.isPending ? (
        <Skeleton variant="card" className="h-40" />
      ) : account.isError ? (
        <Problem error={account.error} what={t("refused.see", t("account.yourPasskeys"))} />
      ) : account.data.member ? (
        <Identity account={account.data} member={account.data.member} />
      ) : (
        <Panel title={t("account.you")}>
          <p className="text-sm text-muted-foreground">
            {account.data.status === "kubernetes"
              ? t("account.kubernetes", account.data.kubernetesUsername)
              : account.data.status === "not-installed"
                ? t("account.notInstalled")
                : t("account.missing", account.data.kubernetesUsername)}
          </p>
        </Panel>
      )}
    </PageColumn>
  )
}

/** Adds a passkey: a one-time link opened in a new tab, since the passkey must be made on
 * Zeabur Identity's own address. */
function useAddPasskey() {
  const [link, setLink] = useState<SetupLink>()
  const add = useMutation({
    mutationFn: async () => {
      // Opened while the click still counts, so a popup blocker lets it through.
      const tab = window.open("", "_blank")
      try {
        const made = await createOwnSetupLink()
        if (tab) {
          tab.opener = null
          tab.location.href = made.url
        }
        return made
      } catch (error) {
        tab?.close()
        throw error
      }
    },
    onSuccess: (made) => {
      setLink(made)
      refreshMembers()
    },
  })
  return { add, link, close: () => setLink(undefined) }
}

function Identity({ account, member }: { account: Account; member: Member }) {
  const t = useT()
  const { add, link, close } = useAddPasskey()
  const ready = linkState(account.setupLinks) === "ready"
  const addButton = (
    <Button
      size="sm"
      variant="outline"
      disabled={!ready}
      loading={add.isPending}
      onClick={() => add.mutate()}
    >
      <PlusIcon aria-hidden="true" data-icon="inline-start" />
      {t("account.addPasskey")}
    </Button>
  )
  return (
    <>
      <Panel title={t("account.you")}>
        <Rows>
          <Row label={t("account.name")}>{memberName(member)}</Row>
          <Row label={t("account.username")}>
            <span className="font-mono text-xs" translate="no">
              {member.username}
            </span>
          </Row>
          <Row label={t("account.role")}>
            {member.role === "admin" ? t("members.role.admin") : t("members.role.member")}
          </Row>
          <Row label={t("account.kubernetesUser")}>
            <span className="font-mono text-xs" translate="no">
              {account.kubernetesUsername}
            </span>
          </Row>
        </Rows>
      </Panel>

      <Panel
        title={t("account.passkeys")}
        action={account.canManage === "denied" ? undefined : addButton}
      >
        <div className="flex flex-col gap-4">
          {needsBackup(member) ? (
            <Alert variant="warn">
              <ShieldWarningIcon aria-hidden="true" />
              <AlertDescription>{t("account.backup")}</AlertDescription>
            </Alert>
          ) : null}
          {member.passkeys.length === 0 ? (
            <Alert variant="warn">
              <ShieldWarningIcon aria-hidden="true" />
              <AlertDescription>{t("account.noPasskey")}</AlertDescription>
            </Alert>
          ) : null}
          <LinksUnavailable links={account.setupLinks} />
          <ManageDenied
            hint={account.canManage}
            what={t("account.adding")}
            denied={t("account.denied")}
          />
          {add.isError ? <Problem error={add.error} /> : null}
          <PasskeyList member={member} own />
        </div>
      </Panel>

      <Panel title={t("account.linked")}>
        <LinkedList member={member} own />
      </Panel>

      {link ? <OwnLinkDialog link={link} onClose={close} /> : null}
    </>
  )
}
