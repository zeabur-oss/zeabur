import { useQuery } from "@tanstack/react-query"
import { Button } from "@zeabur/design-system/components/button"
import { SectionHeader } from "@zeabur/design-system/components/section-header"

import { buildsQuery, projectQuery } from "@/api/queries"
import { GateNote, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { SecurityProfileField, useProjectSecurity } from "@/components/security-profile"
import { useT } from "@/i18n"
import { builtContainers } from "@/model/builds"
import { profileChanges } from "@/model/pod-security"
import type { Service } from "@/model/service"

/** Strict or Compatible for an existing service, switched by a plan; and, once no service needs
 * Compatible, the way back to a Strict project. */
export function SecuritySection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  const { enforced, canChange } = useProjectSecurity(project)
  const services = useQuery(projectQuery(project)).data?.services
  const builds = useQuery(buildsQuery(project)).data?.builds
  const built = builds !== undefined && builtContainers(service, builds).length > 0
  const managed = service.provenance.tool
  const workload = { kind: service.kind, name: service.name }
  const current = service.podSecurity ?? undefined
  const raisable =
    canChange !== "denied" &&
    (enforced === "baseline" || enforced === "privileged") &&
    services !== undefined &&
    services.every((s) => s.podSecurity === "restricted" || s.podSecurity == null)
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
              changes: profileChanges(workload, profile, enforced),
              service: workload,
            })
          }
        />
        <p className="type-meta text-muted-foreground">
          {t("security.project")}
          {t("common.colon")}
          {t(`security.projectLevel.${enforced ?? "unset"}`)}
        </p>
        {raisable ? (
          <div>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                planner.plan({
                  title: t("title.projectStrict"),
                  changes: [{ op: "podSecurity", level: "restricted" }],
                })
              }
            >
              {t("security.raise")}
            </Button>
          </div>
        ) : null}
      </div>
      {planner.dialog}
    </section>
  )
}
