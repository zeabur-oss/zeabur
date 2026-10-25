import { useQuery } from "@tanstack/react-query"
import { SectionHeader } from "@zeabur/design-system/components/section-header"

import { buildsQuery } from "@/api/queries"
import { GateNote, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { SecurityProfileField } from "@/components/security-profile"
import { useT } from "@/i18n"
import { builtContainers } from "@/model/builds"
import { profileChanges } from "@/model/pod-security"
import type { Service } from "@/model/service"

/** Strict or Compatible for an existing service, switched by a plan. */
export function SecuritySection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  const builds = useQuery(buildsQuery(project)).data?.builds
  const built = builds !== undefined && builtContainers(service, builds).length > 0
  const managed = service.provenance.tool
  const workload = { kind: service.kind, name: service.name }
  const current = service.podSecurity ?? undefined
  return (
    <section aria-labelledby="security-title" className="flex flex-col gap-3">
      <SectionHeader
        id="security-title"
        title={t("part.security")}
        description={t("security.hint")}
      />
      {managed ? null : <GateNote gate={gate} />}
      <div className="flex flex-col gap-3 rounded-xl border border-v2-border bg-card p-5">
        <SecurityProfileField
          project={project}
          value={current === "restricted" ? "restricted" : "baseline"}
          current={current}
          built={built}
          disabled={!!managed || !gate.open}
          onChange={(profile) =>
            planner.plan({
              title: t("title.security", service.name),
              changes: profileChanges(workload, profile),
              service: workload,
            })
          }
        />
      </div>
      {planner.dialog}
    </section>
  )
}
