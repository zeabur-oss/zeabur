import { useMutation } from "@tanstack/react-query"
import { useId, useState, type ReactNode } from "react"
import { toast } from "sonner"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { createSetupLink, inviteMember, refreshMembers, type SetupLink } from "@/api/members"
import type { Schemas } from "@/api/client"
import { Problem } from "@/components/problem"
import { RefusedHint, useHintOffersElevation } from "@/components/refused"
import { useI18n, useT } from "@/i18n"
import { formatTime } from "@/lib/format"
import {
  inviteTarget,
  linkState,
  losses,
  memberName,
  type Member,
  type SetupLinks,
} from "@/model/members"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { CopyInput } from "@zeabur/design-system/components/copy-input"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"

/** The frame every dialog here shares with the plan's: title, body, then the buttons. */
function Frame({
  title,
  wide,
  onClose,
  children,
  footer,
}: {
  title: string
  wide?: boolean
  onClose: () => void
  children: ReactNode
  footer: ReactNode
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant={wide ? "wide" : "compact"}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{title}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 py-4 text-sm">
          {children}
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t px-5 py-3">
          {footer}
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** What will be lost, first, in the plan's warning callout. */
function Consequence({ children }: { children: ReactNode }) {
  return (
    <Alert variant="warn" role="note">
      <WarningFillIcon aria-hidden="true" />
      <AlertDescription>{children}</AlertDescription>
    </Alert>
  )
}

/** A confirmation: the consequence first when there is one, the button naming the action. */
export function ConfirmDialog({
  title,
  consequence,
  lines = [],
  action,
  done,
  run,
  onClose,
}: {
  title: string
  consequence?: string
  lines?: string[]
  action: string
  done: string
  run: () => Promise<void>
  onClose: () => void
}) {
  const t = useT()
  const mutation = useMutation({
    mutationFn: run,
    onSuccess: () => {
      toast.success(done)
      refreshMembers()
      onClose()
    },
  })
  return (
    <Frame
      title={title}
      wide={consequence !== undefined}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button
            size="sm"
            variant={consequence !== undefined ? "destructive" : "default"}
            loading={mutation.isPending}
            onClick={() => mutation.mutate()}
          >
            {action}
          </Button>
        </>
      }
    >
      {consequence ? <Consequence>{consequence}</Consequence> : null}
      {lines.map((line) => (
        <p key={line} className={consequence ? "text-muted-foreground" : undefined}>
          {line}
        </p>
      ))}
      {mutation.isError ? <Problem error={mutation.error} /> : null}
    </Frame>
  )
}

/** A link just made: shown once, since only its hash is kept. */
function LinkShown({ link, lead }: { link: SetupLink; lead: string }) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <>
      <p>{lead}</p>
      <CopyInput value={link.url} copyLabel={t("link.copy")} />
      <p className="text-muted-foreground">
        {t("link.expires", formatTime(link.expiresAt, locale))} {t("link.once")}
      </p>
    </>
  )
}

function DoneButton({ onClose }: { onClose: () => void }) {
  const t = useT()
  return (
    <Button size="sm" onClick={onClose}>
      {t("link.done")}
    </Button>
  )
}

export function InviteDialog({
  members,
  onClose,
}: {
  members: readonly Member[]
  onClose: () => void
}) {
  const t = useT()
  const nameId = useId()
  const displayId = useId()
  const adminId = useId()
  const [username, setUsername] = useState("")
  const [displayName, setDisplayName] = useState("")
  const [admin, setAdmin] = useState(false)
  const target = inviteTarget(members, username)
  const invite = useMutation({
    mutationFn: () =>
      inviteMember({
        username: username.trim(),
        displayName: displayName.trim() || null,
        admin,
      }),
    onSuccess: refreshMembers,
  })
  if (invite.data)
    return (
      <Frame title={t("invite.title")} onClose={onClose} footer={<DoneButton onClose={onClose} />}>
        <LinkShown link={invite.data} lead={t("link.send", invite.data.username)} />
      </Frame>
    )
  const typed = username.trim() !== ""
  return (
    <Frame
      title={t("invite.title")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button
            size="sm"
            disabled={target === "invalid"}
            loading={invite.isPending}
            onClick={() => invite.mutate()}
          >
            {t("invite.create")}
          </Button>
        </>
      }
    >
      <p className="text-muted-foreground">{t("invite.body")}</p>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor={nameId}>{t("invite.username")}</Label>
        <Input
          id={nameId}
          mono
          value={username}
          autoComplete="off"
          aria-invalid={typed && target === "invalid"}
          aria-describedby={`${nameId}-hint`}
          onChange={(event) => setUsername(event.target.value)}
        />
        <p id={`${nameId}-hint`} className="type-meta text-muted-foreground">
          {typed && target === "invalid"
            ? t("invite.invalid")
            : target === "existing"
              ? t("invite.existing", username.trim())
              : t("invite.usernameHint")}
        </p>
      </div>
      {target === "existing" ? null : (
        <>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={displayId}>
              {t("invite.displayName")}{" "}
              <span className="font-normal text-muted-foreground">{t("invite.optional")}</span>
            </Label>
            <Input
              id={displayId}
              value={displayName}
              autoComplete="off"
              onChange={(event) => setDisplayName(event.target.value)}
            />
          </div>
          <div className="flex items-start gap-2.5">
            <Checkbox
              id={adminId}
              checked={admin}
              onCheckedChange={(checked) => setAdmin(checked)}
              className="mt-0.5"
            />
            <div className="flex flex-col gap-0.5">
              <Label htmlFor={adminId}>{t("invite.admin")}</Label>
              <span className="type-meta text-muted-foreground">{t("members.adminHint")}</span>
            </div>
          </div>
        </>
      )}
      {invite.isError ? <Problem error={invite.error} /> : null}
    </Frame>
  )
}

/** A member's new link: by default it only adds a passkey; a reset first removes what they have. */
export function SetupLinkDialog({
  member,
  reset: resetFirst = false,
  onClose,
}: {
  member: Member
  reset?: boolean
  onClose: () => void
}) {
  const t = useT()
  const resetId = useId()
  const [reset, setReset] = useState(resetFirst)
  const name = memberName(member)
  const lost = losses(member)
  const parts = [
    lost.passkeys > 0 ? t("reset.passkeys", lost.passkeys) : "",
    lost.linked > 0 ? t("reset.linked", lost.linked) : "",
    lost.sessions > 0 ? t("reset.sessions", lost.sessions) : "",
  ].filter(Boolean)
  const create = useMutation({
    mutationFn: () => createSetupLink(member.username, reset),
    onSuccess: refreshMembers,
  })
  const title = t("reset.title", name)
  if (create.data)
    return (
      <Frame title={title} onClose={onClose} footer={<DoneButton onClose={onClose} />}>
        <LinkShown link={create.data} lead={t("link.send", member.username)} />
      </Frame>
    )
  return (
    <Frame
      title={title}
      wide={reset}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button
            size="sm"
            variant={reset ? "destructive" : "default"}
            loading={create.isPending}
            onClick={() => create.mutate()}
          >
            {reset ? t("reset.resetCreate") : t("invite.create")}
          </Button>
        </>
      }
    >
      {reset ? (
        <Consequence>
          {parts.length > 0
            ? t("reset.consequence", name, t("common.list", parts))
            : t("reset.consequenceNone", name)}
        </Consequence>
      ) : (
        <p>{t("reset.keep", name)}</p>
      )}
      <div className="flex items-start gap-2.5">
        <Checkbox
          id={resetId}
          checked={reset}
          onCheckedChange={(checked) => setReset(checked)}
          className="mt-0.5"
        />
        <Label htmlFor={resetId}>{t("reset.option")}</Label>
      </div>
      {create.isError ? <Problem error={create.error} /> : null}
    </Frame>
  )
}

/** Why no link can be made, or why one made would not work yet. */
export function LinksUnavailable({ links }: { links: SetupLinks }) {
  const t = useT()
  const state = linkState(links)
  if (state === "ready") return null
  return (
    <p className="text-sm text-muted-foreground">
      {state === "no-issuer" ? t("members.noIssuer") : t("members.notRedeemed")}
    </p>
  )
}

/** Why a change is not offered: admin rights would allow it, or the cluster does not. */
export function ManageDenied({
  hint,
  what,
  denied,
}: {
  hint: Schemas["Permission"]
  what: string
  denied: string
}) {
  const offered = useHintOffersElevation(hint)
  if (hint !== "denied") return null
  if (offered)
    return (
      <RefusedHint hint={hint} what={what} missing={["create invitations.identity.zeabur.com"]} />
    )
  return <p className="text-sm text-muted-foreground">{denied}</p>
}
