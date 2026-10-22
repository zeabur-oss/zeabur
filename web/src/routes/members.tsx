import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { toast } from "sonner"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import DotsThreeIcon from "~icons/ph/dots-three"
import LinkSimpleIcon from "~icons/ph/link-simple"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import ShieldWarningIcon from "~icons/ph/shield-warning"
import TrashIcon from "~icons/ph/trash"
import UserGearIcon from "~icons/ph/user-gear"
import UserPlusIcon from "~icons/ph/user-plus"
import XIcon from "~icons/ph/x"

import { membersQuery, refreshMembers, removeMember, revokeInvitation } from "@/api/members"
import { Ago } from "@/components/bits"
import { AccessDialog, useAccessLines } from "@/components/members/access"
import { LinkedList, PasskeyList } from "@/components/members/credentials"
import {
  ConfirmDialog,
  InviteDialog,
  LinksUnavailable,
  ManageDenied,
  SetupLinkDialog,
} from "@/components/members/dialogs"
import { Problem } from "@/components/problem"
import { RefusedSome } from "@/components/refused"
import { useI18n, useT } from "@/i18n"
import { formatAgo, formatTime } from "@/lib/format"
import {
  canChangeAccess,
  canRemoveMember,
  filterMembers,
  linkState,
  memberName,
  membersLayout,
  needsBackup,
  pendingInvitations,
  sortMembers,
  type Invitation,
  type Member,
} from "@/model/members"
import { Avatar } from "@zeabur/design-system/components/avatar"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type Open =
  | { kind: "invite" }
  | { kind: "details"; member: Member }
  | { kind: "link"; member: Member; reset: boolean }
  | { kind: "remove"; member: Member }
  | { kind: "access"; member: Member }

export function MembersPage() {
  const t = useT()
  const members = useQuery(membersQuery)
  const [open, setOpen] = useState<Open>()
  const close = () => setOpen(undefined)
  const data = members.data
  const ready = data ? linkState(data.setupLinks) === "ready" : false
  const projectRoles = data?.projectRoles ?? false
  const invite = (
    <Button size="sm" disabled={!ready} onClick={() => setOpen({ kind: "invite" })}>
      <UserPlusIcon aria-hidden="true" data-icon="inline-start" />
      {t("members.invite")}
    </Button>
  )
  // A member opened before a change is shown as the list now has them.
  const current = (member: Member) =>
    data?.items.find((m) => m.username === member.username) ?? member

  return (
    <PageColumn gap="lg" className="mt-6 mb-12 md:mt-10">
      <PageHeader
        title={t("members.title")}
        actions={data?.installed && data.canManage !== "denied" ? invite : undefined}
      />
      {members.isPending ? (
        <Skeleton variant="card" className="h-50" />
      ) : members.isError ? (
        <Problem error={members.error} what={t("refused.see", t("members.everyone"))} />
      ) : !members.data.installed ? (
        <Empty variant="outline">
          <EmptyHeader>
            <EmptyMedia>
              <Illustration kind="empty" />
            </EmptyMedia>
            <EmptyTitle>{t("members.notInstalled")}</EmptyTitle>
            <EmptyDescription>{t("members.notInstalledBody")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <>
          <LinksUnavailable links={members.data.setupLinks} />
          <ManageDenied
            hint={members.data.canManage}
            what={t("members.inviting")}
            denied={t("members.denied")}
          />
          <MemberList items={members.data.items} projectRoles={projectRoles} onOpen={setOpen} />
          <RefusedSome unreadable={members.data.unreadable} />
          <Pending invitations={members.data.invitations} />
        </>
      )}
      {open?.kind === "invite" ? (
        <InviteDialog members={data?.items ?? []} projectRoles={projectRoles} onClose={close} />
      ) : open?.kind === "details" ? (
        <MemberPanel
          member={current(open.member)}
          projectRoles={projectRoles}
          onOpen={setOpen}
          onClose={close}
        />
      ) : open?.kind === "access" ? (
        <AccessDialog member={current(open.member)} projectRoles={projectRoles} onClose={close} />
      ) : open?.kind === "link" ? (
        <SetupLinkDialog member={current(open.member)} reset={open.reset} onClose={close} />
      ) : open?.kind === "remove" ? (
        <RemoveMember member={current(open.member)} onClose={close} />
      ) : null}
    </PageColumn>
  )
}

/** Every member, one line each; a filter above the table once there are many. */
function MemberList({
  items,
  projectRoles,
  onOpen,
}: {
  items: Member[]
  projectRoles: boolean
  onOpen: (open: Open) => void
}) {
  const t = useT()
  const [query, setQuery] = useState("")
  const layout = membersLayout(items.length)
  if (layout === "empty")
    return <p className="text-sm text-muted-foreground">{t("members.none")}</p>
  const shown = filterMembers(sortMembers(items), query)
  return (
    <section className="flex flex-col gap-3" aria-label={t("members.title")}>
      {layout === "table" ? (
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring sm:w-64">
          <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label={t("members.filter")}
            placeholder={t("members.filter")}
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </label>
      ) : null}
      <div className="overflow-hidden rounded-xl border border-main bg-card text-sm">
        <div
          aria-hidden="true"
          className="hidden grid-cols-member items-center gap-4 border-b border-main bg-table-head px-4 py-2 type-meta text-secondary md:grid"
        >
          <span>{t("members.column.name")}</span>
          <span>{t("members.column.role")}</span>
          <span>{t("members.column.passkeys")}</span>
          <span>{t("members.column.lastSignIn")}</span>
          <span />
        </div>
        {shown.length === 0 ? (
          <p className="px-4 py-3 text-muted-foreground">{t("members.noMatch")}</p>
        ) : (
          <ul className="divide-y divide-main">
            {shown.map((member) => (
              <MemberRow
                key={member.username}
                member={member}
                projectRoles={projectRoles}
                onOpen={onOpen}
              />
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}

function PasskeyCount({ member }: { member: Member }) {
  const t = useT()
  const count = member.passkeys.length
  const words = count === 0 ? t("members.noPasskey") : t("members.passkeys", count)
  if (count > 1) return <span className="text-muted-foreground">{words}</span>
  return (
    <Tooltip content={count === 0 ? t("members.noPasskeyHint") : t("members.onePasskeyHint")}>
      <span className="inline-flex items-center gap-1 text-warn-strong">
        <ShieldWarningIcon aria-hidden="true" className="size-3.5" />
        {words}
      </span>
    </Tooltip>
  )
}

function MemberRow({
  member,
  projectRoles,
  onOpen,
}: {
  member: Member
  projectRoles: boolean
  onOpen: (open: Open) => void
}) {
  const t = useT()
  const accessLines = useAccessLines()
  const name = memberName(member)
  const access = member.disabled ? [t("members.disabled")] : accessLines(member)
  return (
    <li className="grid grid-cols-member-phone items-center gap-3 px-4 py-2.5 md:grid-cols-member md:gap-4">
      <button
        type="button"
        onClick={() => onOpen({ kind: "details", member })}
        className="flex min-w-0 items-center gap-3 text-left outline-none"
      >
        <Avatar size="sm" shape="circle" name={name} />
        <span className="flex min-w-0 flex-col">
          <span className="flex min-w-0 items-baseline gap-1.5">
            <span className="truncate font-medium hover:underline" title={name}>
              {name}
            </span>
            {member.you ? (
              <span className="shrink-0 type-meta text-muted-foreground">{t("members.you")}</span>
            ) : null}
          </span>
          <span
            className="truncate font-mono text-xs text-muted-foreground max-md:hidden"
            translate="no"
          >
            {member.username}
          </span>
        </span>
      </button>
      <span className="flex min-w-0 flex-col text-muted-foreground max-md:hidden">
        {access.map((line) => (
          <span key={line} className="truncate" title={line}>
            {line}
          </span>
        ))}
      </span>
      <span className="whitespace-nowrap">
        <PasskeyCount member={member} />
      </span>
      <span className="max-md:hidden">
        {member.lastSignInAt ? <Ago iso={member.lastSignInAt} /> : <span>—</span>}
      </span>
      <span className="flex justify-end">
        <MemberMenu member={member} projectRoles={projectRoles} onOpen={onOpen} />
      </span>
    </li>
  )
}

function MemberMenu({
  member,
  projectRoles,
  onOpen,
}: {
  member: Member
  projectRoles: boolean
  onOpen: (open: Open) => void
}) {
  const t = useT()
  const name = memberName(member)
  return (
    <Menu>
      <MenuTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={t("members.actions", name)} />}
      >
        <DotsThreeIcon aria-hidden="true" />
      </MenuTrigger>
      <MenuContent align="end">
        {canChangeAccess(member, projectRoles) ? (
          <MenuItem onClick={() => onOpen({ kind: "access", member })}>
            <UserGearIcon aria-hidden="true" />
            {t("roles.change")}
          </MenuItem>
        ) : null}
        <MenuItem onClick={() => onOpen({ kind: "link", member, reset: false })}>
          <LinkSimpleIcon aria-hidden="true" />
          {t("members.newLink")}
        </MenuItem>
        {member.you ? null : (
          <MenuItem onClick={() => onOpen({ kind: "link", member, reset: true })}>
            <ArrowCounterClockwiseIcon aria-hidden="true" />
            {t("members.reset")}
          </MenuItem>
        )}
        {canRemoveMember(member) ? (
          <MenuItem onClick={() => onOpen({ kind: "remove", member })}>
            <TrashIcon aria-hidden="true" />
            {t("members.remove")}
          </MenuItem>
        ) : null}
      </MenuContent>
    </Menu>
  )
}

/** One member: their passkeys and linked accounts, and what can be done about them. */
function MemberPanel({
  member,
  projectRoles,
  onOpen,
  onClose,
}: {
  member: Member
  projectRoles: boolean
  onOpen: (open: Open) => void
  onClose: () => void
}) {
  const t = useT()
  const accessLines = useAccessLines()
  const name = memberName(member)
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-3 border-b px-5 py-3">
          <Avatar size="sm" shape="circle" name={name} />
          <div className="flex min-w-0 flex-1 flex-col">
            <DialogTitle>{name}</DialogTitle>
            <span className="truncate font-mono text-xs text-muted-foreground" translate="no">
              zeabur:{member.username}
            </span>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-5 py-4">
          <dl className="grid grid-cols-pairs gap-x-4 gap-y-2 text-sm">
            <dt className="text-muted-foreground">{t("account.role")}</dt>
            <dd className="flex min-w-0 flex-col">
              {accessLines(member).map((line) => (
                <span key={line}>{line}</span>
              ))}
              {member.disabled ? <span>{t("members.disabled")}</span> : null}
            </dd>
            <dt className="text-muted-foreground">{t("members.column.lastSignIn")}</dt>
            <dd>{member.lastSignInAt ? <Ago iso={member.lastSignInAt} /> : "—"}</dd>
            <dt className="text-muted-foreground">{t("members.sessions")}</dt>
            <dd className="tabular-nums">{member.sessions}</dd>
          </dl>
          {member.projects && member.projects.length > 0 ? (
            <section className="flex flex-col gap-3">
              <h3 className="text-sm font-medium">
                {t("roles.projectsHeading")}{" "}
                <span className="font-normal text-muted-foreground tabular-nums">
                  {member.projects.length}
                </span>
              </h3>
              <ul className="max-h-64 divide-y divide-main overflow-y-auto rounded-lg border border-main text-sm">
                {member.projects.map((grant) => (
                  <li key={grant.project} className="flex items-center gap-3 px-3 py-2">
                    <span className="min-w-0 flex-1 truncate font-mono" translate="no">
                      {grant.project}
                    </span>
                    <span className="text-muted-foreground">{t(`roles.${grant.role}`)}</span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          <section className="flex flex-col gap-3">
            <h3 className="text-sm font-medium">{t("account.passkeys")}</h3>
            {needsBackup(member) && member.you ? (
              <p className="text-sm text-warn-strong">{t("account.backup")}</p>
            ) : null}
            <PasskeyList member={member} own={member.you} />
          </section>
          <section className="flex flex-col gap-3">
            <h3 className="text-sm font-medium">{t("account.linked")}</h3>
            <LinkedList member={member} own={member.you} />
          </section>
        </div>
        <div className="flex flex-wrap items-center gap-2 border-t px-5 py-3">
          {canChangeAccess(member, projectRoles) ? (
            <Button size="sm" variant="outline" onClick={() => onOpen({ kind: "access", member })}>
              <UserGearIcon aria-hidden="true" data-icon="inline-start" />
              {t("roles.change")}
            </Button>
          ) : null}
          <Button
            size="sm"
            variant="outline"
            onClick={() => onOpen({ kind: "link", member, reset: false })}
          >
            <LinkSimpleIcon aria-hidden="true" data-icon="inline-start" />
            {t("members.newLink")}
          </Button>
          {member.you ? null : (
            <Button
              size="sm"
              variant="outline"
              onClick={() => onOpen({ kind: "link", member, reset: true })}
            >
              {t("members.reset")}
            </Button>
          )}
          {canRemoveMember(member) ? (
            <Button
              size="sm"
              variant="secondary-destructive"
              className="ml-auto"
              onClick={() => onOpen({ kind: "remove", member })}
            >
              {t("members.remove")}
            </Button>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function RemoveMember({ member, onClose }: { member: Member; onClose: () => void }) {
  const t = useT()
  const name = memberName(member)
  return (
    <ConfirmDialog
      title={t("remove.title", name)}
      consequence={t("remove.consequence", name)}
      lines={[t("remove.stays", `zeabur:${member.username}`)]}
      action={t("remove.action", name)}
      done={t("remove.done", name)}
      run={() => removeMember(member.username)}
      onClose={onClose}
    />
  )
}

/** Links made and not yet used, soonest to expire first. */
function Pending({ invitations }: { invitations: Invitation[] }) {
  const t = useT()
  const { locale } = useI18n()
  const pending = pendingInvitations(invitations, Date.now())
  const revoke = useMutation({
    mutationFn: revokeInvitation,
    onSuccess: () => {
      toast.success(t("members.revoked"))
      refreshMembers()
    },
  })
  if (pending.length === 0) return null
  return (
    <section className="flex flex-col gap-3">
      <h2 className="type-heading">{t("members.pending")}</h2>
      <ul className="divide-y divide-main overflow-hidden rounded-xl border border-main bg-card text-sm">
        {pending.map((invitation) => (
          <li
            key={invitation.name}
            className="grid grid-cols-invitation-phone items-center gap-x-4 gap-y-0.5 px-4 py-2.5 md:grid-cols-member"
          >
            <span className="truncate font-mono text-xs" translate="no">
              {invitation.username}
            </span>
            <span className="text-muted-foreground max-md:col-start-1 max-md:row-start-2">
              {invitation.newMember ? t("members.pendingNew") : t("members.pendingAdds")}
            </span>
            <span className="text-muted-foreground max-md:hidden md:col-span-2">
              <Tooltip content={formatTime(invitation.expiresAt, locale)}>
                <time dateTime={invitation.expiresAt}>
                  {t("members.expires", formatAgo(invitation.expiresAt, locale))}
                </time>
              </Tooltip>
            </span>
            <Button
              size="xs"
              variant="ghost"
              className="justify-self-end max-md:row-span-2 max-md:col-start-2 max-md:row-start-1"
              loading={revoke.isPending && revoke.variables === invitation.name}
              onClick={() => revoke.mutate(invitation.name)}
            >
              {t("members.revoke")}
            </Button>
          </li>
        ))}
      </ul>
      {revoke.isError ? <Problem error={revoke.error} /> : null}
    </section>
  )
}
