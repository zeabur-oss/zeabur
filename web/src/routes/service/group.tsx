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

/** The group the sidebar lists the service in, by the group label on the workload. */
export function GroupSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const managed = service.provenance.tool
  const groups = groupNames(useQuery(projectQuery(project)).data?.services ?? [])
  const [typed, setTyped] = useState(service.grouped ? service.group : "")
  const disabled = !!managed || !gate.open
  const change = disabled ? undefined : groupChange(service, typed)
  const workload = { kind: service.kind, name: service.name }
  useHold("group", change ? { changes: [change], scope: { ops: ["group"], workload } } : undefined)
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
            onChange={(event) => setTyped(event.target.value)}
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
