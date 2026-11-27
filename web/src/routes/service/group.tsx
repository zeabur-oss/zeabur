import { useQuery } from "@tanstack/react-query"
import { Input } from "@zeabur-oss/design-system/components/input"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import { useId, useState } from "react"

import { projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { GateNote, useGate } from "@/components/gate"
import { useHold } from "@/components/plan/draft"
import { useT } from "@/i18n"
import type { Service } from "@/model/service"
import { groupChange, groupNames, validGroup } from "@/model/service-group"

/**
 * The group name in the field. It holds a change only after the person types, so a form that
 * mounts before the project refresh does not stage the old group as an edit.
 */
function useGroupEdit(service: Service, disabled: boolean) {
  const [edited, setEdited] = useState<string>()
  const change = disabled || edited === undefined ? undefined : groupChange(service, edited)
  const workload = { kind: service.kind, name: service.name }
  useHold("group", change ? { changes: [change], scope: { ops: ["group"], workload } } : undefined)
  return [edited ?? (service.grouped ? service.group : ""), setEdited] as const
}

/** The group the sidebar lists the service in, by the group label on the workload. */
export function GroupSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const managed = service.provenance.tool
  const groups = groupNames(useQuery(projectQuery(project)).data?.services ?? [])
  const disabled = !!managed || !gate.open
  const [typed, setEdited] = useGroupEdit(service, disabled)
  const list = useId()
  const name = typed.trim()
  return (
    <section aria-labelledby="group-title" className="flex flex-col gap-3">
      <SectionHeader id="group-title" title={t("part.group")} description={t("group.hint")} />
      {managed ? null : <GateNote gate={gate} />}
      <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
        <FormField
          label={t("group.name")}
          hint={t("group.nameHint")}
          errors={name && !validGroup(name) ? [t("group.invalid")] : []}
          disabled={disabled}
        >
          <Input
            size="sm"
            list={list}
            value={typed}
            placeholder={t("group.placeholder")}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setEdited(event.target.value)}
          />
          <datalist id={list}>
            {groups.map((group) => (
              <option key={group} value={group} />
            ))}
          </datalist>
        </FormField>
      </div>
    </section>
  )
}
