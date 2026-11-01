import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import UserPlusIcon from "~icons/ph/user-plus"

import { membersQuery } from "@/api/members"
import { InviteDialog } from "@/components/members/dialogs"
import { GateTip, useHintGate } from "@/components/gate"
import { useT } from "@/i18n"
import { linkState, memberName, projectPeople } from "@/model/members"
import { Avatar } from "@zeabur-oss/design-system/components/avatar"
import { Button, buttonVariants } from "@zeabur-oss/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur-oss/design-system/components/card"

const SHOWN = 7

/** Who can open a project, and an invitation with the project already picked. Hidden for a
 * namespace that is not a project, and when the members cannot be read or Zeabur Identity is not
 * installed. */
export function ProjectMembers({ namespace }: { namespace: { name: string; adopted: boolean } }) {
  const project = namespace.name
  const t = useT()
  const members = useQuery(membersQuery)
  const [inviting, setInviting] = useState(false)
  const manage = useHintGate("changeMembers", members.data?.canManage)
  const data = members.data
  if (!namespace.adopted || !data?.installed) return null
  const people = projectPeople(data.items, project)
  const more = people.length - SHOWN
  return (
    <section>
      <SectionLabel>{t("members.title")}</SectionLabel>
      <p className="mb-3 type-meta text-muted-foreground">{t("projectMembers.about")}</p>
      <Card variant="list">
        {people.slice(0, SHOWN).map(({ member, role }) => (
          <CardRow
            key={member.username}
            label={
              <span className="flex min-w-0 items-center gap-2.5">
                <Avatar size="sm" shape="circle" name={memberName(member)} />
                <span className="truncate">{memberName(member)}</span>
                {member.you ? (
                  <span className="shrink-0 font-normal text-secondary">{t("members.you")}</span>
                ) : null}
              </span>
            }
          >
            <span className="text-sm text-secondary">
              {role === "admin" ? t("members.role.admin") : t(`roles.${role}`)}
            </span>
          </CardRow>
        ))}
        {more > 0 ? (
          <CardRow label={t("projectMembers.more", more)}>
            <Link to="/members" className={buttonVariants({ variant: "outline", size: "sm" })}>
              {t("projectMembers.all")}
            </Link>
          </CardRow>
        ) : null}
        <CardRow
          label={t("projectMembers.invite")}
          description={data.projectRoles ? t("projectMembers.inviteHint") : undefined}
        >
          <GateTip gate={manage}>
            <Button
              variant="outline"
              size="sm"
              disabled={linkState(data.setupLinks) !== "ready"}
              onClick={() => setInviting(true)}
            >
              <UserPlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("members.invite")}
            </Button>
          </GateTip>
        </CardRow>
      </Card>
      {inviting ? (
        <InviteDialog
          members={data.items}
          projectRoles={data.projectRoles}
          project={project}
          onClose={() => setInviting(false)}
        />
      ) : null}
    </section>
  )
}
