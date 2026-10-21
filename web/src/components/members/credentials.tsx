import { useMutation } from "@tanstack/react-query"
import { useState } from "react"
import { toast } from "sonner"
import GithubLogoIcon from "~icons/ph/github-logo"
import GoogleLogoIcon from "~icons/ph/google-logo"
import KeyIcon from "~icons/ph/key"
import LinkSimpleIcon from "~icons/ph/link-simple"

import { refreshMembers, removeCredential, removeOwnPasskey, renamePasskey } from "@/api/members"
import { Ago } from "@/components/bits"
import { ConfirmDialog } from "@/components/members/dialogs"
import { Problem } from "@/components/problem"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { memberName, passkeyKept, type Member, type Passkey } from "@/model/members"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type Linked = Member["linked"][number]

/** When it was added and last used, as far as Zeabur Identity records them. */
function Dates({
  createdAt,
  lastUsedAt,
}: {
  createdAt?: string | null
  lastUsedAt?: string | null
}) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <span className="flex flex-wrap gap-x-3 type-meta text-muted-foreground">
      {createdAt ? <span>{t("passkey.added", formatDate(createdAt, locale))}</span> : null}
      {lastUsedAt ? (
        <span>
          {t("passkey.lastUsed")} <Ago iso={lastUsedAt} />
        </span>
      ) : null}
    </span>
  )
}

function PasskeyRow({ member, passkey, own }: { member: Member; passkey: Passkey; own: boolean }) {
  const t = useT()
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(passkey.label ?? "")
  const [removing, setRemoving] = useState(false)
  const name = passkey.label ?? t("passkey.unnamed")
  const kept = passkeyKept(member)
  const rename = useMutation({
    mutationFn: () => renamePasskey(passkey.id, draft),
    onSuccess: () => {
      toast.success(t("passkey.renamed"))
      setEditing(false)
      refreshMembers()
    },
  })
  return (
    <li className="flex flex-col gap-2 py-3 first:pt-0 last:pb-0">
      <div className="flex items-center gap-3">
        <KeyIcon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
        {editing ? (
          <form
            className="flex min-w-0 flex-1 items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              rename.mutate()
            }}
          >
            <Input
              size="sm"
              value={draft}
              maxLength={64}
              autoFocus
              aria-label={t("passkey.nameLabel")}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") setEditing(false)
              }}
              className="min-w-0 flex-1"
            />
            <Button size="xs" type="submit" loading={rename.isPending}>
              {t("passkey.save")}
            </Button>
            <Button size="xs" variant="ghost" type="button" onClick={() => setEditing(false)}>
              {t("plan.cancel")}
            </Button>
          </form>
        ) : (
          <>
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate font-medium" title={name}>
                {name}
              </span>
              <Dates createdAt={passkey.createdAt} lastUsedAt={passkey.lastUsedAt} />
            </div>
            {own ? (
              <Button
                size="xs"
                variant="ghost"
                onClick={() => {
                  setDraft(passkey.label ?? "")
                  setEditing(true)
                }}
              >
                {t("passkey.rename")}
              </Button>
            ) : null}
            {kept ? (
              <Tooltip content={t("passkey.lastKept")}>
                <span tabIndex={0}>
                  <Button size="xs" variant="ghost" disabled>
                    {t("passkey.remove")}
                  </Button>
                </span>
              </Tooltip>
            ) : (
              <Button size="xs" variant="ghost" onClick={() => setRemoving(true)}>
                {t("passkey.remove")}
              </Button>
            )}
          </>
        )}
      </div>
      {rename.isError ? <Problem error={rename.error} /> : null}
      {removing ? (
        <ConfirmDialog
          title={t("passkey.removeTitle", name)}
          consequence={
            own ? t("passkey.removeOwn", name) : t("passkey.removeOther", name, memberName(member))
          }
          lines={[t("passkey.sessionsStay")]}
          action={t("passkey.removeAction")}
          done={t("passkey.removed")}
          run={() =>
            own ? removeOwnPasskey(passkey.id) : removeCredential(member.username, passkey.id)
          }
          onClose={() => setRemoving(false)}
        />
      ) : null}
    </li>
  )
}

/** A member's passkeys, oldest first; `own` when they are yours, which can also be renamed. */
export function PasskeyList({ member, own }: { member: Member; own: boolean }) {
  const t = useT()
  if (member.passkeys.length === 0)
    return <p className="text-sm text-muted-foreground">{t("passkey.none")}</p>
  return (
    <ul className="flex flex-col divide-y divide-main text-sm">
      {member.passkeys.map((passkey) => (
        <PasskeyRow key={passkey.id} member={member} passkey={passkey} own={own} />
      ))}
    </ul>
  )
}

function ProviderIcon({ provider }: { provider: string }) {
  const Icon =
    provider === "GitHub" ? GithubLogoIcon : provider === "Google" ? GoogleLogoIcon : LinkSimpleIcon
  return <Icon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground" />
}

function LinkedRow({ member, linked, own }: { member: Member; linked: Linked; own: boolean }) {
  const t = useT()
  const [removing, setRemoving] = useState(false)
  return (
    <li className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
      <ProviderIcon provider={linked.provider} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="font-medium" translate="no">
          {linked.provider}
        </span>
        <Dates createdAt={linked.createdAt} lastUsedAt={linked.lastUsedAt} />
      </div>
      <Button size="xs" variant="ghost" onClick={() => setRemoving(true)}>
        {t("linked.remove")}
      </Button>
      {removing ? (
        <ConfirmDialog
          title={t("linked.removeTitle", linked.provider)}
          lines={[
            own
              ? t("linked.removeOwn", linked.provider)
              : t("linked.removeOther", memberName(member), linked.provider),
          ]}
          action={t("linked.remove")}
          done={t("linked.removed")}
          run={() =>
            own ? removeOwnPasskey(linked.id) : removeCredential(member.username, linked.id)
          }
          onClose={() => setRemoving(false)}
        />
      ) : null}
    </li>
  )
}

/** GitHub or Google accounts linked for everyday sign-in. */
export function LinkedList({ member, own }: { member: Member; own: boolean }) {
  const t = useT()
  if (member.linked.length === 0)
    return <p className="text-sm text-muted-foreground">{t("linked.none")}</p>
  return (
    <ul className="flex flex-col divide-y divide-main text-sm">
      {member.linked.map((linked) => (
        <LinkedRow key={linked.id} member={member} linked={linked} own={own} />
      ))}
    </ul>
  )
}
