import { useMutation, useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import { toast } from "sonner"

import { refreshMembers, setMemberAccess } from "@/api/members"
import { projectsQuery } from "@/api/queries"
import { Consequence, Frame } from "@/components/members/frame"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import {
  accessLosses,
  accessSummary,
  chosenOf,
  grantableProjects,
  grantsOf,
  memberName,
  pickerHasFilter,
  pickerProjects,
  type Member,
  type ProjectRole,
} from "@/model/members"
import { projectTitle } from "@/model/project"
import { Button } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"
import { Skeleton } from "@zeabur/design-system/components/skeleton"

export type Chosen = ReadonlyMap<string, ProjectRole>

/** A project by its title, with its namespace after it when the two differ. */
export function ProjectLabel({ name, title }: { name: string; title: string }) {
  if (title === name)
    return (
      <span className="truncate font-mono" translate="no" title={name}>
        {name}
      </span>
    )
  return (
    <span className="flex min-w-0 items-baseline gap-2" title={`${title} (${name})`}>
      <span className="truncate" translate="no">
        {title}
      </span>
      <span className="truncate font-mono type-meta text-muted-foreground" translate="no">
        {name}
      </span>
    </span>
  )
}

/** A project's name as the sidebar shows it; its namespace until the projects load. */
export function useProjectTitle(): (project: string) => string {
  const projects = useQuery(projectsQuery)
  return (project) =>
    projectTitle(
      projects.data?.items.find((item) => item.name === project),
      project,
    )
}

/** A member's rights in a few words, a line per role: admin, each role with up to three projects
 * named, or none. */
export function useAccessLines(): (member: Pick<Member, "role" | "projects">) => string[] {
  const t = useT()
  const title = useProjectTitle()
  return (member) => {
    const summary = accessSummary(member, title)
    switch (summary.kind) {
      case "admin":
        return [t("members.role.admin")]
      case "unknown":
        return [t("members.role.member")]
      case "none":
        return [t("roles.none")]
      case "projects":
        return summary.roles.map(({ role, named, count }) =>
          named.length > 0
            ? t("roles.in", t(`roles.${role}`), t("common.list", named))
            : t("roles.inCount", t(`roles.${role}`), count),
        )
    }
  }
}

/** Admin, or a role in each chosen project. Without the member roles, admin or nothing. */
export function AccessFields({
  projectRoles,
  admin,
  onAdmin,
  chosen,
  onChosen,
  held = [],
}: {
  projectRoles: boolean
  admin: boolean
  onAdmin: (admin: boolean) => void
  chosen: Chosen
  onChosen: (chosen: Chosen) => void
  held?: string[]
}) {
  const t = useT()
  const adminId = useId()
  if (!projectRoles)
    return (
      <div className="flex items-start gap-2.5">
        <Checkbox
          id={adminId}
          checked={admin}
          onCheckedChange={(checked) => onAdmin(checked)}
          className="mt-0.5"
        />
        <div className="flex flex-col gap-0.5">
          <Label htmlFor={adminId}>{t("invite.admin")}</Label>
          <span className="type-meta text-muted-foreground">{t("members.adminHint")}</span>
        </div>
      </div>
    )
  return (
    <fieldset className="flex min-w-0 flex-col gap-2">
      <legend className="mb-1.5 text-sm font-medium">{t("roles.legend")}</legend>
      <RadioGroup
        value={admin ? "admin" : "projects"}
        onValueChange={(value) => onAdmin(value === "admin")}
      >
        <RadioOption
          value="projects"
          label={t("roles.projects")}
          description={t("roles.projectsHint")}
        />
        <RadioOption value="admin" label={t("invite.admin")} description={t("members.adminHint")} />
      </RadioGroup>
      {admin ? null : <ProjectPicker chosen={chosen} onChosen={onChosen} held={held} />}
    </fieldset>
  )
}

/** Every project with a box to pick it and, once picked, its role; a filter once there are many. */
function ProjectPicker({
  chosen,
  onChosen,
  held,
}: {
  chosen: Chosen
  onChosen: (chosen: Chosen) => void
  held: string[]
}) {
  const t = useT()
  const id = useId()
  const projects = useQuery(projectsQuery)
  const [query, setQuery] = useState("")
  const [first] = useState(() => new Set(chosen.keys()))
  if (projects.isPending) return <Skeleton variant="card" className="h-24" />
  if (projects.isError) return <Problem error={projects.error} />
  const names = grantableProjects(projects.data.items, held)
  const title = (name: string) =>
    projectTitle(
      projects.data.items.find((item) => item.name === name),
      name,
    )
  const shown = pickerProjects(names, chosen, query, first, title)
  const pick = (name: string, role: ProjectRole | undefined) => {
    const next = new Map(chosen)
    if (role) next.set(name, role)
    else next.delete(name)
    onChosen(next)
  }
  const roles = [
    { value: "developer" as const, label: t("roles.developer") },
    { value: "viewer" as const, label: t("roles.viewer") },
  ]
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <ul className="flex flex-col gap-1 type-meta text-muted-foreground">
        <li>{t("roles.developerHint")}</li>
        <li>{t("roles.viewerHint")}</li>
      </ul>
      {names.length === 0 ? (
        <p className="text-muted-foreground">{t("roles.noProjects")}</p>
      ) : (
        <>
          {pickerHasFilter(names.length) ? (
            <Input
              type="search"
              value={query}
              aria-label={t("roles.filter")}
              placeholder={t("roles.filter")}
              onChange={(event) => setQuery(event.target.value)}
            />
          ) : null}
          <ul
            aria-label={t("roles.projects")}
            className="max-h-64 divide-y divide-main overflow-y-auto rounded-lg border border-main"
          >
            {shown.length === 0 ? (
              <li className="px-3 py-2 text-muted-foreground">{t("roles.noMatch")}</li>
            ) : (
              shown.map((name) => {
                const role = chosen.get(name)
                return (
                  <li key={name} className="flex h-10 min-w-0 items-center gap-2.5 px-3">
                    <Checkbox
                      id={`${id}-${name}`}
                      checked={role !== undefined}
                      onCheckedChange={(checked) => pick(name, checked ? "viewer" : undefined)}
                    />
                    <Label htmlFor={`${id}-${name}`} className="min-w-0 flex-1">
                      <ProjectLabel name={name} title={title(name)} />
                    </Label>
                    {role ? (
                      <Select
                        size="sm"
                        items={roles}
                        value={role}
                        onValueChange={(next) => pick(name, next)}
                        aria-label={t("roles.roleIn", title(name))}
                      />
                    ) : null}
                  </li>
                )
              })
            )}
          </ul>
          <p className="type-meta text-muted-foreground">
            {chosen.size === 0 ? t("roles.noneChosen") : t("roles.chosen", chosen.size)}
          </p>
        </>
      )}
    </div>
  )
}

/** Change a member's access: what they lose first, then the choice. */
export function AccessDialog({
  member,
  projectRoles,
  onClose,
}: {
  member: Member
  projectRoles: boolean
  onClose: () => void
}) {
  const t = useT()
  const name = memberName(member)
  const [admin, setAdmin] = useState(member.role === "admin")
  const [chosen, setChosen] = useState<Chosen>(() => chosenOf(member))
  const held = (member.projects ?? []).map((p) => p.project)
  const title = useProjectTitle()
  const lost = accessLosses(member, admin, chosen)
  const titles = (names: string[]) => t("common.list", names.map(title))
  const consequences = [
    lost.admin ? t("roles.losesAdmin", name) : "",
    lost.removed.length > 0 ? t("roles.losesProjects", name, titles(lost.removed)) : "",
    lost.lowered.length > 0 ? t("roles.losesChanges", name, titles(lost.lowered)) : "",
  ].filter(Boolean)
  const save = useMutation({
    mutationFn: () =>
      setMemberAccess(member.username, {
        admin,
        projects: admin || !projectRoles ? [] : grantsOf(chosen),
      }),
    onSuccess: () => {
      toast.success(t("roles.saved", name))
      refreshMembers()
      onClose()
    },
  })
  return (
    <Frame
      title={t("roles.title", name)}
      wide
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button
            size="sm"
            variant={consequences.length > 0 ? "destructive" : "default"}
            loading={save.isPending}
            onClick={() => save.mutate()}
          >
            {t("roles.save")}
          </Button>
        </>
      }
    >
      {consequences.length > 0 ? (
        <Consequence>
          {consequences.join(" ")} {t("roles.openTerminals")}
        </Consequence>
      ) : !lost.admin && admin && member.role !== "admin" ? (
        <Consequence>{t("roles.becomesAdmin", name)}</Consequence>
      ) : null}
      <AccessFields
        projectRoles={projectRoles}
        admin={admin}
        onAdmin={setAdmin}
        chosen={chosen}
        onChosen={setChosen}
        held={held}
      />
      {save.isError ? <Problem error={save.error} /> : null}
    </Frame>
  )
}
